export default function Terms() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Terms of Use</h1>
        <p className="text-neutral-400 text-sm">Last updated: March 25, 2026</p>

        <Section title="1. Acceptance of Terms">
          By downloading, installing, or using Codync (&quot;the App&quot;), you agree to
          be bound by these Terms of Use. If you do not agree, do not use the App.
        </Section>

        <Section title="2. Description of Service">
          Codync is a real-time monitoring companion app for AI coding sessions.
          It displays session metadata (status, model, tasks, costs) synced via
          iCloud between your Mac and iPhone/Apple Watch.
        </Section>

        <Section title="3. Subscriptions">
          <p>
            Codync offers an optional auto-renewable subscription plan:
          </p>
          <ul className="list-disc pl-5 space-y-2 mt-3">
            <li>
              <strong>Codync Pro</strong> — $0.99/month
            </li>
          </ul>
          <p className="mt-3">Codync Pro includes:</p>
          <ul className="list-disc pl-5 space-y-2 mt-2">
            <li>Background push updates — Live Activities stay current even when the app is closed</li>
            <li>Always-on Dynamic Island — persistent session status on your Lock Screen</li>
            <li>Instant APNs delivery — real-time updates with zero delay</li>
          </ul>
          <div className="mt-4 space-y-2">
            <p>Payment will be charged to your Apple ID account at confirmation of purchase.</p>
            <p>Subscription automatically renews unless it is canceled at least 24 hours before the end of the current period.</p>
            <p>Your account will be charged for renewal within 24 hours prior to the end of the current period at the same price.</p>
            <p>You can manage and cancel your subscriptions by going to your account settings on the App Store after purchase.</p>
          </div>
        </Section>

        <Section title="4. Free Features">
          The App can be used for free with core features including real-time
          session monitoring, iCloud sync, and Live Activity display. No account
          or subscription is required for basic functionality.
        </Section>

        <Section title="5. User Responsibilities">
          <ul className="list-disc pl-5 space-y-2">
            <li>You must have a valid Apple ID and iCloud account for data sync.</li>
            <li>You are responsible for maintaining the security of your devices and accounts.</li>
            <li>You agree not to misuse the App or attempt to reverse-engineer its services.</li>
          </ul>
        </Section>

        <Section title="6. Intellectual Property">
          All content, design, and code in the App are owned by Pokai Lee. You
          are granted a limited, non-exclusive license to use the App for
          personal, non-commercial purposes.
        </Section>

        <Section title="7. Disclaimer of Warranties">
          The App is provided &quot;as is&quot; without warranty of any kind. We do not
          guarantee uninterrupted or error-free operation. Session data accuracy
          depends on the source application and iCloud availability.
        </Section>

        <Section title="8. Limitation of Liability">
          To the maximum extent permitted by law, Pokai Lee shall not be liable
          for any indirect, incidental, or consequential damages arising from
          your use of the App.
        </Section>

        <Section title="9. Changes to Terms">
          We may update these Terms from time to time. Continued use of the App
          after changes constitutes acceptance of the new Terms. Changes will be
          posted on this page with an updated revision date.
        </Section>

        <Section title="10. Contact">
          If you have questions about these Terms, contact us at{" "}
          <a href="mailto:kevin2005ha@gmail.com" className="text-white underline">
            kevin2005ha@gmail.com
          </a>.
        </Section>

        <div className="pt-4 text-neutral-400 leading-relaxed">
          <p>
            <a href="/privacy" className="text-white underline">Privacy Policy</a>
          </p>
        </div>

        <div className="pt-4">
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
