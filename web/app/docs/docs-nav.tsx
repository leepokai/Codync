"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { docSections } from "./pages";

// The manual's sidebar: sections of pages, the current one marked.
export default function DocsNav() {
  const path = usePathname();
  return (
    <nav aria-label="Docs" className="shrink-0 md:sticky md:top-24 md:w-56 md:self-start">
      <Link href="/docs" className="text-sm font-semibold text-neutral-50">
        Codync docs
      </Link>
      <div className="mt-5 grid grid-cols-2 gap-x-6 gap-y-6 md:grid-cols-1">
        {docSections.map((section) => (
          <div key={section.title}>
            <p className="text-xs font-medium tracking-wide text-neutral-500 uppercase">{section.title}</p>
            <ul className="mt-2 space-y-1">
              {section.pages.map((page) => {
                const href = `/docs/${page.slug}`;
                const current = path === href || path === `${href}/`;
                return (
                  <li key={page.slug}>
                    <Link
                      href={href}
                      aria-current={current ? "page" : undefined}
                      className={`block py-1 text-sm transition ${current ? "text-neutral-50" : "text-neutral-400 hover:text-neutral-200"}`}
                    >
                      {page.title}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </div>
    </nav>
  );
}
