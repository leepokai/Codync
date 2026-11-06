import type { Metadata } from "next";
import Link from "next/link";
import { docSections } from "./pages";

export const metadata: Metadata = {
  title: "Docs",
  description:
    "How Codync works and how to use it: bots, agents, conversations, group chats, routines, memory, connectors, remote access and privacy.",
  alternates: { canonical: "/docs" },
};

export default function DocsIndex() {
  return (
    <article>
      <h1 className="text-4xl font-semibold tracking-tighter text-neutral-50 md:text-5xl">Codync docs</h1>
      <p className="mt-5 max-w-[38rem] text-lg leading-relaxed text-neutral-400">
        Codync turns the coding agents on your computer into bots you message from your iPhone, your desktop or a
        terminal. This manual explains how bots work and how to set them up.
      </p>
      <div className="mt-12 grid gap-10 sm:grid-cols-2">
        {docSections.map((section) => (
          <section key={section.title}>
            <h2 className="text-xs font-medium tracking-wide text-neutral-500 uppercase">{section.title}</h2>
            <ul className="mt-3 space-y-2">
              {section.pages.map((page) => (
                <li key={page.slug}>
                  <Link href={`/docs/${page.slug}`} className="text-neutral-200 underline-offset-4 transition hover:text-white hover:underline">
                    {page.title}
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </article>
  );
}
