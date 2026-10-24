import type { SignInProvider } from './analytics'

export const signInProviderName: Record<SignInProvider, string> = {
  apple: 'Apple',
  google: 'Google',
  wechat: 'WeChat',
}

/** Clerk's Frontend API strategy. A custom OIDC connection is `oauth_custom_<key>`. */
export function clerkStrategy(provider: SignInProvider): string {
  return provider === 'wechat' ? 'oauth_custom_wechat' : `oauth_${provider}`
}
