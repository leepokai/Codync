import type { Metadata } from "next";
import Link from "next/link";

export const metadata: Metadata = { title: "Privacy Policy", alternates: { canonical: "/privacy" } };

export default function Privacy() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Privacy Policy</h1>
        <p className="text-neutral-400 text-sm">Last updated: October 13, 2026</p>

        <Section title="Overview">
          Codync lets you message the AI agents that run on your own computer. It is built so that your files, conversations and credentials stay on your devices.
        </Section>

        <Section title="What stays on your devices">
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>Bots and conversations</strong> are stored by the Codync host on your computer (in <code>~/.codync</code>). Your phone keeps a cache so the app opens instantly.</li>
            <li><strong>Your phone talks directly to your computer</strong> over your local network or Tailscale. There is no Codync server in between and no Codync account.</li>
            <li><strong>Agents run with your own logins</strong> (Claude Code, Codex, Cursor and others). Codync never sees or stores their credentials.</li>
            <li><strong>Usage limits</strong> are read locally from the agents installed on your computer. Only percentages reach your phone.</li>
          </ul>
        </Section>

        <Section title="Push notifications">
          To notify you when a bot needs you or finishes, your computer sends a short alert (the bot&apos;s name and a one-line preview) through our push relay, a Cloudflare Worker that forwards it to Apple Push Notification service. The relay does not store notifications. Your device token is encrypted into a ticket that only the relay can read; your computer never sees the raw token.
        </Section>

        <Section title="Data we collect">
          <p className="mb-2">
            Only usage analytics. They are on by default; turn them off on each iPhone and each computer in Settings (Share usage analytics), and that device stops sending right away.
          </p>
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>What we record:</strong> which features you use, such as opening the app, creating or deleting a bot, sending a message (with the number of attachments, not the files), starting a routine or a voice call, and installing a connector from the catalog. Each event carries the app version, the platform and the agent and model a bot uses.</li>
            <li><strong>Crash and error reports:</strong> when an app crashes or hits an unexpected error, or an agent fails to answer, we record what kind of error it was and where in Codync&apos;s own code it happened (type, function names and line numbers). The error&apos;s message is never sent, and file paths are cut down to Codync&apos;s own files.</li>
            <li><strong>What we never record:</strong> your messages, prompts, code, files, file names or paths, bot names, credentials or anything your agents produce. We don&apos;t store your IP address: it is only used to look up your approximate location (country and city), which is kept with the events.</li>
            <li><strong>Who it&apos;s linked to:</strong> your Codync account when you&apos;re signed in; otherwise a random ID created on that device.</li>
            <li><strong>Why:</strong> to spot bugs sooner and serve you better, by seeing which features work and where people get stuck. It is never sold, never shared for advertising and never used to track you across other apps or websites.</li>
          </ul>
          <p className="mt-2">Codync has no advertising identifiers.</p>
        </Section>

        <Section title="Third-party services">
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>Apple Push Notification service</strong>: delivers notifications and Live Activity updates.</li>
            <li><strong>Cloudflare Workers</strong>: runs the push relay (nothing stored).</li>
            <li><strong>PostHog (US)</strong>: stores the usage analytics described above, unless you turned them off.</li>
            <li><strong>The AI agents you choose</strong>: they run on your computer under their own terms and privacy policies.</li>
            <li><strong>OpenAI or Google (optional, voice calls)</strong>: only if you add your own OpenAI or Gemini API key in a call&apos;s settings. Your voice then goes from your iPhone or Mac directly to that provider, under your account and its privacy policy, together with the bot&apos;s name and description and the chat messages it asks for during the call. The key is stored encrypted on your computer, which uses it only to start calls and list models. Without a key, voice calls use Apple speech recognition on your device.</li>
          </ul>
          <p className="mt-2">This policy covers Codync as we publish it. Codync services run by third parties (for example a hosted host or relay) handle your data under their own privacy policies; we don&apos;t operate them and receive none of that data.</p>
        </Section>

        <Section title="Data retention">
          Everything lives on your devices. Delete a bot to remove its conversation; uninstall the host and delete <code>~/.codync</code> to remove all of it. Deleting the iPhone app removes its cache. Turning analytics off stops new events from being sent.
        </Section>

        <Section title="Children's privacy">
          Codync is not directed at children under the age of 13.
        </Section>

        <Section title="Changes">
          We may update this policy. Changes will be posted on this page with a new date.
        </Section>

        <Section title="Contact">
          Questions? Open an issue at{" "}
          <a href="https://github.com/leepokai/Codync/issues" className="text-white underline">github.com/leepokai/Codync/issues</a>.
        </Section>

        <div className="pt-4">
          <p className="text-neutral-400">
            <Link href="/terms" className="text-white underline">Terms of Use</Link>
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
