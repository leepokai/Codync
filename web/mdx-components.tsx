import type { MDXComponents } from "mdx/types";
import Link from "next/link";

// The user manual's typography (content/docs/*.mdx) in the site's dark palette; the page's title is its h1
// (app/docs/[slug]/page.tsx). Headings carry the ids rehype-slug gives them, so `/docs/bots#new-session` links work.
const components: MDXComponents = {
  h2: ({ children, id }) => (
    <h2 id={id} className="mt-14 scroll-mt-24 text-2xl font-semibold tracking-tight text-balance text-neutral-50">
      {children}
    </h2>
  ),
  h3: ({ children, id }) => (
    <h3 id={id} className="mt-9 scroll-mt-24 text-lg font-semibold text-balance text-neutral-100">
      {children}
    </h3>
  ),
  p: ({ children }) => <p className="mt-5 max-w-[42rem] leading-relaxed text-neutral-400">{children}</p>,
  ul: ({ children }) => <ul className="mt-5 max-w-[42rem] list-disc space-y-2 pl-5 leading-relaxed text-neutral-400 marker:text-neutral-600">{children}</ul>,
  ol: ({ children }) => <ol className="mt-5 max-w-[42rem] list-decimal space-y-2 pl-5 leading-relaxed text-neutral-400 marker:text-neutral-500">{children}</ol>,
  li: ({ children }) => <li className="pl-1 [&>ol]:mt-2 [&>p]:mt-2 [&>ul]:mt-2">{children}</li>,
  strong: ({ children }) => <strong className="font-semibold text-neutral-200">{children}</strong>,
  a: ({ href = "", children }) =>
    href.startsWith("/") || href.startsWith("#") ? (
      <Link href={href} className="text-neutral-100 underline underline-offset-4 hover:text-white">
        {children}
      </Link>
    ) : (
      <a href={href} className="text-neutral-100 underline underline-offset-4 hover:text-white">
        {children}
      </a>
    ),
  code: ({ children }) => <code className="rounded bg-neutral-900 px-1.5 py-0.5 font-mono text-[0.85em] break-words text-neutral-200">{children}</code>,
  pre: ({ children }) => (
    <pre className="mt-5 max-w-[42rem] overflow-x-auto rounded-xl bg-neutral-900 p-4 text-sm leading-relaxed text-neutral-200 [&>code]:bg-transparent [&>code]:p-0">
      {children}
    </pre>
  ),
  blockquote: ({ children }) => <blockquote className="mt-5 max-w-[42rem] border-l-2 border-neutral-600 pl-4 [&>p]:mt-0">{children}</blockquote>,
  table: ({ children }) => (
    <div className="mt-5 max-w-full overflow-x-auto">
      <table className="w-full min-w-[32rem] border-collapse text-left text-sm text-neutral-400">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border-b border-neutral-700 py-2 pr-4 font-medium text-neutral-200">{children}</th>,
  td: ({ children }) => <td className="border-b border-neutral-800 py-2 pr-4 align-top">{children}</td>,
  hr: () => <hr className="mt-10 border-neutral-800" />,
};

export function useMDXComponents(): MDXComponents {
  return components;
}
