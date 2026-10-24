import * as Sentry from '@sentry/nextjs'

const enabled = process.env.NEXT_PUBLIC_KOHARU_PREVIEW !== '1'

if (enabled) {
  Sentry.init({
    dsn: process.env.NEXT_PUBLIC_SENTRY_DSN,
  })
}

export const onRouterTransitionStart: typeof Sentry.captureRouterTransitionStart = (...args) => {
  if (enabled) Sentry.captureRouterTransitionStart(...args)
}
