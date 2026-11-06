import type { Metadata } from "next";
import Link from "next/link";
import { EMAIL, GITHUB } from "../links";

export const metadata: Metadata = { title: "Terms of Use", alternates: { canonical: "/terms" } };

export default function Terms() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Terms of Use</h1>
        <p className="text-neutral-400 text-sm">Last updated: October 24, 2026</p>

        <Section title="1. Acceptance of Terms">
          By using Codync you agree to these terms. If you don&apos;t agree, don&apos;t use the app.
        </Section>

        <Section title="2. The service">
          Codync is free, open-source software (Apache License 2.0) that lets you message AI agents running on your own computer from your iPhone, Android phone, Mac, Windows PC or Linux desktop.
        </Section>

        <Section title="3. Your responsibility">
          <ul className="list-disc pl-5 space-y-2">
            <li>Agents act on your computer with your permissions. Review approval requests before allowing them; &quot;Approve automatically&quot; lets agents act without asking.</li>
            <li>Keep your pairing code private. Anyone with it can run agents on your computer; reset it with <code>codync-host reset-token</code>.</li>
            <li>You are responsible for complying with the terms of the agents and services you connect.</li>
          </ul>
        </Section>

        <Section title="4. Codync Cloud">
          <ul className="list-disc pl-5 space-y-2">
            <li>Codync Cloud (accounts, the relay at api.codync.dev, push notifications and remote-screen relay) is offered free for personal use: your own computers and devices.</li>
            <li>Hosted, resold or multi-tenant services built on Codync must run their own relay and accounts (the source is in <code>cloud/</code>; point hosts at it with <code>CODYNC_CLOUD_URL</code>). They may not use Codync Cloud.</li>
            <li>We may rate-limit, suspend or block any computer or account that breaks these terms or puts unusual load on the service.</li>
          </ul>
        </Section>

        <Section title="5. Third-party services">
          Anyone may run Codync for others under the Apache License 2.0. Services operated by third parties, including hosted hosts, relays, accounts or bundled AI access built on Codync, are not operated, endorsed or supported by us. Their operators are solely responsible for their service, security, billing, support and the data they handle; your agreement with them is between you and them. We are not liable for any third-party service.
        </Section>

        <Section title="6. Intellectual property">
          The source code is available at github.com/leepokai/Codync under the Apache License 2.0. The license covers the code, not the Codync name or logo (Apache 2.0 section 6): services built on Codync may say they are built on it, but may not call themselves Codync. Names and logos of third-party agents belong to their owners.
        </Section>

        <Section title="7. Disclaimer of warranties">
          Codync is provided &quot;as is&quot;, without warranties of any kind. We don&apos;t guarantee that agents will behave as intended or that the service will be uninterrupted.
        </Section>

        <Section title="8. Limitation of liability">
          To the maximum extent permitted by law, we are not liable for any damages arising from your use of Codync or of the agents it runs, including changes they make to your files.
        </Section>

        <Section title="9. Changes">
          We may update these terms. Continued use after changes means you accept them.
        </Section>

        <Section title="10. Contact">
          Questions? Email{" "}
          <a href={`mailto:${EMAIL}`} className="text-white underline">{EMAIL}</a> or open an issue at{" "}
          <a href={`${GITHUB}/issues`} className="text-white underline">github.com/leepokai/Codync/issues</a>.
        </Section>

        <div className="pt-4">
          <p className="text-neutral-400">
            <Link href="/privacy" className="text-white underline">Privacy Policy</Link>
          </p>
        </div>

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
