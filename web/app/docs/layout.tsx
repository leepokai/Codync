import { SiteFooter, SiteHeader } from "../components/site-chrome";
import DocsNav from "./docs-nav";

// Every manual page: the site's header and footer around a sidebar of pages and the page itself.
export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <>
      <SiteHeader />
      <div className="mx-auto flex w-full max-w-6xl flex-1 flex-col gap-10 px-4 py-10 sm:px-6 md:flex-row md:gap-14 md:py-16">
        <DocsNav />
        <main className="min-w-0 flex-1">{children}</main>
      </div>
      <SiteFooter />
    </>
  );
}
