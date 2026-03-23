export default function Privacy() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Privacy Policy</h1>
        <p className="text-neutral-400 text-sm">Last updated: March 23, 2026</p>

        <Section title="Overview">
          Codync is a real-time monitoring tool for Claude Code sessions. We are
          committed to protecting your privacy and being transparent about the
          data we handle.
        </Section>

        <Section title="Data Collection">
          <p>Codync does <strong>not</strong> collect, store, or transmit any personal data to our servers.</p>
          <ul className="list-disc pl-5 space-y-2 mt-2">
            <li><strong>Session data</strong> (project names, status, tasks, costs) is synced between your Mac and iPhone exclusively through your personal iCloud account (CloudKit). We never see this data.</li>
            <li><strong>Code content</strong> is never read, stored, or transmitted. Codync only reads session metadata (status, model, task names).</li>
            <li><strong>Push notifications</strong> for Pro subscribers are relayed through a Cloudflare Worker that forwards Live Activity updates via Apple Push Notification service (APNs). The Worker processes session status data transiently and does not store it.</li>
          </ul>
        </Section>

        <Section title="iCloud Sync">
          All data synchronization between your devices uses Apple CloudKit with your personal iCloud account. This data is governed by{" "}
          <a href="https://www.apple.com/legal/privacy/" className="text-white underline">Apple&apos;s Privacy Policy</a>.
        </Section>

        <Section title="Subscriptions">
          Codync Pro is an optional auto-renewable subscription managed entirely through the App Store. Payment processing is handled by Apple. We use RevenueCat to manage subscription status — RevenueCat receives an anonymous app user ID and subscription status, but no personal information.
        </Section>

        <Section title="Third-Party Services">
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>Apple CloudKit</strong> — iCloud sync between your devices</li>
            <li><strong>Apple APNs</strong> — Push notifications for Live Activity updates</li>
            <li><strong>RevenueCat</strong> — Subscription management (anonymous user ID only)</li>
            <li><strong>Cloudflare Workers</strong> — APNs push relay (no data stored)</li>
          </ul>
        </Section>

        <Section title="Data Retention">
          Session data in CloudKit is automatically deleted when sessions end. No historical data is retained. Deleting the app removes all local data. iCloud data can be managed through your iCloud settings.
        </Section>

        <Section title="Children's Privacy">
          Codync is not directed at children under the age of 13 and we do not knowingly collect information from children.
        </Section>

        <Section title="Changes">
          We may update this policy from time to time. Changes will be posted on this page with an updated revision date.
        </Section>

        <Section title="Contact">
          If you have questions, contact us at{" "}
          <a href="mailto:kevin2005ha@gmail.com" className="text-white underline">kevin2005ha@gmail.com</a>.
        </Section>

        <div className="pt-8">
          <a href="/" className="text-sm text-neutral-500 hover:text-neutral-300 transition-colors">
            &larr; Back to home
          </a>
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
