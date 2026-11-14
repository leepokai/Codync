import type { Metadata } from "next";
import Link from "next/link";
import { GITHUB } from "../links";

export const metadata: Metadata = { title: "Code Signing Policy", alternates: { canonical: "/code-signing" } };

const MAINTAINER = "https://github.com/leepokai";

// Required by SignPath Foundation's open source program (signpath.org/terms).
export default function CodeSigning() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Code signing policy</h1>
        <p className="text-neutral-400 leading-relaxed">
          Free code signing provided by{" "}
          <a href="https://about.signpath.io" className="text-white underline">SignPath.io</a>, certificate by{" "}
          <a href="https://signpath.org" className="text-white underline">SignPath Foundation</a>.
        </p>

        <Section title="What is signed">
          The Windows releases of the Codync desktop app: the installer and the executables Codync builds itself. They
          are built by GitHub Actions on GitHub-hosted runners from the source code at{" "}
          <a href={GITHUB} className="text-white underline">github.com/leepokai/Codync</a>, and each release is signed
          only after an approver approves it. Third-party programs the app includes, such as the Electron runtime and the
          computer-use driver, are never signed with this certificate. Signing starts with the first Windows release after
          SignPath Foundation accepts the project; earlier releases are unsigned.
        </Section>

        <Section title="Team roles">
          <ul className="list-disc pl-5 space-y-2">
            <li>
              <strong>Committers and reviewers:</strong>{" "}
              <a href={MAINTAINER} className="text-white underline">leepokai</a>. Changes from anyone else, such as pull
              requests, are reviewed before they are merged.
            </li>
            <li>
              <strong>Approvers:</strong> <a href={MAINTAINER} className="text-white underline">leepokai</a>, who approves
              every signing request by hand.
            </li>
          </ul>
        </Section>

        <Section title="Privacy">
          The data Codync sends, and the services it uses, are described in the{" "}
          <Link href="/privacy" className="text-white underline">Privacy Policy</Link>. The Windows installer explains
          the usage analytics, links that policy and lets you turn analytics off before anything is sent; you can also
          turn them off later in Settings.
        </Section>

        <Section title="Reporting">
          If you find a signed file that was not built from this repository, or any other security issue, report it as
          described in <a href={`${GITHUB}/blob/main/SECURITY.md`} className="text-white underline">SECURITY.md</a>.
        </Section>

        <div className="pt-4">
          <Link href="/" className="text-sm text-neutral-500 hover:text-neutral-300 transition-colors">
            &larr; Back to home
          </Link>
        </div>
      </article>
    </main>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="text-xl font-semibold text-white mb-2">{title}</h2>
      <div className="text-neutral-400 leading-relaxed">{children}</div>
    </section>
  );
}
