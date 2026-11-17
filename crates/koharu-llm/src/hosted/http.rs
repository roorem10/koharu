//! The HTTP client hosted models go through: rig's own, shared by the process,
//! sending a 429 or 5xx reply again, up to three times, after the wait rig reads
//! from it, or a second, then two, then four when it names none; rig itself
//! retries nothing. For a vendor the user signs in to, it also puts the
//! sign-in's current credential on each request, so a long-lived model
//! outlives any one token.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use backon::{ExponentialBuilder, Retryable};
use bytes::Bytes;
use futures::future::BoxFuture;
use http::request::Parts;
use rig_core::{
    error::ProviderError,
    http_client::{self, HttpClientExt, LazyBody, MultipartForm, StreamingResponse},
};
use rig_reqwest::ReqwestClient;

/// What a vendor the user signs in to puts on each request: the sign-in's
/// credential, refreshed first when it has expired, and whatever else the
/// vendor's endpoint requires of a signed-in request.
pub(crate) trait SignIn: Send + Sync {
    fn authorize<'a>(&'a self, parts: &'a mut Parts) -> BoxFuture<'a, Result<()>>;
}

#[derive(Clone, Default)]
pub(crate) struct Http {
    client: ReqwestClient,
    sign_in: Option<Arc<dyn SignIn>>,
}

/// A second, then two, then four.
const BACKOFF: ExponentialBuilder = ExponentialBuilder::new();

/// rig's own client, for the requests Koharu makes outside rig.
pub(crate) fn reqwest() -> Result<reqwest::Client> {
    ReqwestClient::default()
        .inner()
        .cloned()
        .context("couldn't build the HTTP client")
}

/// Whether `error` is a 429 or 5xx reply, worth sending again.
fn retryable(error: &http_client::Error) -> bool {
    matches!(
        error,
        http_client::Error::InvalidStatusCodeWithDetails { status, .. }
            if status.as_u16() == 429 || status.is_server_error()
    )
}

/// The wait `error` names in place of `backoff`'s, until `backoff` runs out.
fn retry_after(error: &http_client::Error, backoff: Option<Duration>) -> Option<Duration> {
    let backoff = backoff?;
    let http_client::Error::InvalidStatusCodeWithDetails {
        status,
        body,
        headers,
    } = error
    else {
        return Some(backoff);
    };
    let report = ProviderError::from(http_client::Error::InvalidStatusCodeWithDetails {
        status: *status,
        body: body.clone(),
        headers: headers.clone(),
    })
    .report();
    Some(report.retry_after(SystemTime::now()).unwrap_or(backoff))
}

impl Http {
    /// A client whose requests carry `sign_in`'s credential.
    pub(crate) fn signed_in(sign_in: impl SignIn + 'static) -> Self {
        Self {
            client: ReqwestClient::default(),
            sign_in: Some(Arc::new(sign_in)),
        }
    }

    /// `parts` as the sign-in authorizes them.
    #[expect(clippy::result_large_err, reason = "rig's HTTP error is rig's")]
    async fn authorized(&self, mut parts: Parts) -> http_client::Result<Parts> {
        if let Some(sign_in) = &self.sign_in {
            sign_in
                .authorize(&mut parts)
                .await
                .map_err(|error| http_client::Error::Instance(error.into()))?;
        }
        Ok(parts)
    }
}

impl HttpClientExt for Http {
    fn send<T, U>(
        &self,
        request: http::Request<T>,
    ) -> impl Future<Output = http_client::Result<http::Response<LazyBody<U>>>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        let http = self.clone();
        let (parts, body) = request.into_parts();
        let body: Bytes = body.into();
        async move {
            (|| async {
                let parts = http.authorized(parts.clone()).await?;
                http.client
                    .send(http::Request::from_parts(parts, body.clone()))
                    .await
            })
            .retry(BACKOFF)
            .when(retryable)
            .adjust(retry_after)
            .await
            // Booharu: no address in a failed send, nor in a failed read of
            // the reply, where reqwest names it too.
            .map(|reply| {
                reply.map(|body| -> LazyBody<U> {
                    Box::pin(async move { body.await.map_err(without_url) })
                })
            })
            .map_err(without_url)
        }
    }

    fn send_multipart<U>(
        &self,
        request: http::Request<MultipartForm>,
    ) -> impl Future<Output = http_client::Result<http::Response<LazyBody<U>>>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        self.client.send_multipart(request)
    }

    fn send_streaming<T>(
        &self,
        request: http::Request<T>,
    ) -> impl Future<Output = http_client::Result<StreamingResponse>> + Send
    where
        T: Into<Bytes> + Send,
    {
        let http = self.clone();
        let (parts, body) = request.into_parts();
        let body: Bytes = body.into();
        async move {
            (|| async {
                let parts = http.authorized(parts.clone()).await?;
                http.client
                    .send_streaming(http::Request::from_parts(parts, body.clone()))
                    .await
            })
            .retry(BACKOFF)
            .when(retryable)
            .adjust(retry_after)
            .await
            // Booharu: no address in a failed send; reqwest names none in a
            // streamed chunk's error.
            .map_err(without_url)
        }
    }
}

/// Booharu: `error` without the address reqwest names in a transport error,
/// which rig shows and which can carry a key or a token.
fn without_url(error: http_client::Error) -> http_client::Error {
    match error {
        http_client::Error::Instance(error) => match error.downcast::<reqwest::Error>() {
            Ok(error) => http_client::Error::instance(error.without_url()),
            Err(error) => http_client::Error::Instance(error),
        },
        error => error,
    }
}

/// Booharu: Gemini's API key, which rig puts in the address as `key=`,
/// where a log or an error that names the address shows it, sent in the
/// `x-goog-api-key` header Gemini also reads. rig puts the key in as it is,
/// unencoded, so it moves as it is; an address without one is left alone.
struct GeminiKey;

impl SignIn for GeminiKey {
    fn authorize<'a>(&'a self, parts: &'a mut Parts) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let Some(query) = parts.uri.query() else {
                return Ok(());
            };
            let mut key = None;
            let mut kept = Vec::new();
            for pair in query.split('&') {
                match pair.split_once('=') {
                    Some(("key", value)) => key = Some(value),
                    _ => kept.push(pair),
                }
            }
            let Some(key) = key else {
                return Ok(());
            };
            let mut value = http::HeaderValue::from_str(key)?;
            value.set_sensitive(true);
            let path = parts.uri.path();
            let mut uri = parts.uri.clone().into_parts();
            uri.path_and_query = Some(match kept.as_slice() {
                [] => path.parse()?,
                kept => format!("{path}?{}", kept.join("&")).parse()?,
            });
            parts.uri = http::Uri::from_parts(uri)?;
            parts.headers.insert("x-goog-api-key", value);
            Ok(())
        })
    }
}

impl Http {
    /// Booharu: a client for Gemini, sending its API key in a header.
    pub(crate) fn gemini() -> Self {
        Self::signed_in(GeminiKey)
    }
}

#[cfg(test)]
mod tests {
    use http::header::AUTHORIZATION;

    use super::*;

    #[tokio::test]
    async fn another_providers_request_is_left_alone() {
        let (parts, ()) = http::Request::post("https://example.com")
            .header(AUTHORIZATION, "Bearer key")
            .body(())
            .unwrap()
            .into_parts();
        let parts = Http::default().authorized(parts).await.unwrap();
        assert_eq!(parts.headers[AUTHORIZATION], "Bearer key");
    }

    /// Booharu: Gemini's key leaves the address for its header, and the rest
    /// of the address stays as it was.
    #[tokio::test]
    async fn gemini_sends_its_key_in_a_header() {
        let models = "https://g.test/v1beta/models";
        for (uri, sent, key) in [
            (
                format!("{models}/m:streamGenerateContent?alt=sse&key=AIzaFAKE"),
                format!("{models}/m:streamGenerateContent?alt=sse"),
                Some("AIzaFAKE"),
            ),
            (
                format!("{models}?pageSize=1000&key=AIzaFAKE&pageToken=next"),
                format!("{models}?pageSize=1000&pageToken=next"),
                Some("AIzaFAKE"),
            ),
            (
                format!("{models}/m:generateContent?key=AIzaFAKE"),
                format!("{models}/m:generateContent"),
                Some("AIzaFAKE"),
            ),
            (
                format!("{models}?pageSize=1000&pageToken=next"),
                format!("{models}?pageSize=1000&pageToken=next"),
                None,
            ),
        ] {
            let (parts, ()) = http::Request::get(uri).body(()).unwrap().into_parts();
            let parts = Http::gemini().authorized(parts).await.unwrap();
            assert_eq!(parts.uri.to_string(), sent);
            let header = parts.headers.get("x-goog-api-key");
            assert_eq!(header.map(|key| key.to_str().unwrap()), key);
        }
    }

    /// Booharu: a request that can't be sent, or whose reply breaks off,
    /// fails naming neither its address nor the key in it.
    #[tokio::test]
    async fn a_transport_error_names_no_address() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let request = |uri: &str| http::Request::get(uri).body(Bytes::new()).unwrap();
        let unaddressed = |error: http_client::Error| {
            let debug = format!("{error:?}");
            let text = format!("{debug} {:#}", anyhow::Error::from(error));
            assert!(
                !text.contains("v1beta") && !text.contains("AIzaFAKE"),
                "{text}"
            );
        };

        // Nothing listens on the discard port.
        let unreachable = "http://127.0.0.1:9/v1beta/models?key=AIzaFAKE";
        let sent = Http::default().send::<_, Bytes>(request(unreachable)).await;
        unaddressed(sent.err().unwrap());
        let streamed = Http::default().send_streaming(request(unreachable)).await;
        unaddressed(streamed.err().unwrap());

        // A reply shorter than its length says.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broken = format!(
            "http://{}/v1beta/models?key=AIzaFAKE",
            listener.local_addr().unwrap()
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            assert!(socket.read(&mut [0; 1024]).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 9\r\n\r\ncut")
                .await
                .unwrap();
        });
        let reply = Http::default().send::<_, Bytes>(request(&broken)).await;
        unaddressed(reply.unwrap().into_body().await.unwrap_err());
    }
}
