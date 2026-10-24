import type { NextConfig } from 'next'

const nextConfig: NextConfig = {
  devIndicators: false,
  transpilePackages: ['@koharu/bridge', '@koharu/ui'],
  output: 'export',
  images: {
    unoptimized: true,
  },
}

export default async function configuration(): Promise<NextConfig> {
  // A private preview must not construct Sentry's build plugin or upload source maps.
  if (process.env.NEXT_PUBLIC_KOHARU_PREVIEW === '1') return nextConfig
  const { withSentryConfig } = await import('@sentry/nextjs/config')
  return withSentryConfig(nextConfig, {
    org: process.env.SENTRY_ORG,
    project: 'nextjs',
    authToken: process.env.SENTRY_AUTH_TOKEN,
    silent: !process.env.CI,
  })
}
