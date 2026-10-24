import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { docPages } from "../pages";

type Props = { params: Promise<{ slug: string }> };

/** The page `params` names, with the pages before and after it in the sidebar. */
async function find(params: Props["params"]) {
  const { slug } = await params;
  const index = docPages.findIndex((p) => p.slug === slug);
  if (index < 0) notFound();
  return { page: docPages[index], previous: docPages[index - 1], next: docPages[index + 1] };
}

export function generateStaticParams() {
  return docPages.map(({ slug }) => ({ slug }));
}

export const dynamicParams = false;

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { page } = await find(params);
  return { title: page.title, description: page.description, alternates: { canonical: `/docs/${page.slug}` } };
}

export default async function DocPage({ params }: Props) {
  const { page, previous, next } = await find(params);
  const { default: Content } = await import(`@/content/docs/${page.slug}.mdx`);
  return (
    <article>
      <h1 className="text-4xl font-semibold tracking-tighter text-balance text-neutral-50 md:text-5xl">{page.title}</h1>
      <Content />
      <nav aria-label="More docs" className="mt-16 flex justify-between gap-6 border-t border-neutral-800 pt-6 text-sm">
        {previous ? (
          <Link href={`/docs/${previous.slug}`} className="text-neutral-400 transition hover:text-neutral-100">
            &larr; {previous.title}
          </Link>
        ) : (
          <span />
        )}
        {next ? (
          <Link href={`/docs/${next.slug}`} className="text-right text-neutral-400 transition hover:text-neutral-100">
            {next.title} &rarr;
          </Link>
        ) : null}
      </nav>
    </article>
  );
}
