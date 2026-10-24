import type { Metadata } from "next";
import Link from "next/link";
import { EMAIL, GITHUB } from "../links";

export const metadata: Metadata = { title: "Privacy Policy", alternates: { canonical: "/privacy" } };

export default function Privacy() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Privacy Policy</h1>
        <p className="text-neutral-400 text-sm">Last updated: October 24, 2026</p>

        <Section title="Overview">
          Codync lets you message the AI agents that run on your own computer. It is built so that your files, conversations and credentials stay on your devices.
        </Section>

        <Section title="What stays on your devices">
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>Bots and conversations</strong> are stored by the Codync host on your computer (in <code>~/.codync</code>). Your phone keeps a cache so the app opens instantly.</li>
            <li><strong>Your phone talks to your computer</strong> directly over your local network or Tailscale, or, away from home, through our relay, which only forwards end-to-end encrypted data it can&apos;t read. A message you send while your computer is offline waits in the relay, still encrypted for your computer, for up to 24 hours, and is deleted once delivered.</li>
            <li><strong>Agents run with your own logins</strong> (Claude Code, Codex, Cursor and others). Codync never sees or stores their credentials.</li>
            <li><strong>Usage limits</strong> are read locally from the agents installed on your computer. Only percentages reach your phone.</li>
          </ul>
        </Section>

        <Section title="Push notifications">
          To notify you when a bot needs you or finishes, your computer sends a short alert (the bot&apos;s name and a one-line preview) through our push relay, a Cloudflare Worker that forwards it to Apple Push Notification service (iPhone) or Firebase Cloud Messaging (Android). The alert is encrypted for your phone, so the relay and those services carry only generic text, and the relay does not store notifications. Your device token is encrypted into a ticket that only the relay can read; your computer never sees the raw token.
        </Section>

        <Section title="Routine webhooks">
          A routine can start when another service sends it a webhook at its public address on our relay. These deliveries are not end-to-end encrypted: the relay receives them over HTTPS, can read their content, and keeps each one until your computer collects it, for up to 72 hours. It also stores the routine&apos;s key to check signatures. With &ldquo;Reach from anywhere&rdquo; off, deliveries no longer reach your computer, but the relay still accepts them at the old public address and keeps them, readable, for up to 72 hours; they run if you turn it back on within that time. To stop them, remove the address from the sender, or delete the routine or replace its key while &ldquo;Reach from anywhere&rdquo; is on.
        </Section>

        <Section title="Codync account (optional)">
          You can sign in with Apple or Google to find your computers from any phone. Your account keeps your email address, an account ID, and the names of the phones and computers you connect with it. Codync works without an account.
        </Section>

        <Section title="Data we collect">
          <p className="mb-2">
            Usage analytics; the registration of each computer that uses our relay (its name, platform, device type, Codync version, public keys and when it was last online), with the public keys of the devices allowed to use it; and, if you sign in, the account data described under Codync account. Analytics are on by default; turn them off on each phone and each computer in Settings (Share usage analytics), and that device stops sending right away.
          </p>
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>What we record:</strong> which features you use, such as opening the app and its screens, setting up and pairing a device (and whether you allow notifications), creating or deleting a bot, sending a message (with the number of attachments, not the files), starting a routine or a voice call, and installing a connector from the catalog. Each event carries the app version, the platform and the agent and model a bot uses.</li>
            <li><strong>On codync.dev:</strong> page views and clicks on download and outbound links, counted without cookies; counting them stores nothing on your device.</li>
            <li><strong>Crash and error reports:</strong> when an app crashes or hits an unexpected error, or an agent fails to answer, we record what kind of error it was and where in Codync&apos;s own code it happened (type, function names and line numbers). The error&apos;s message is never sent, and file paths are cut down to Codync&apos;s own files.</li>
            <li><strong>What we never record:</strong> your messages, prompts, code, files, file names or paths, bot names, credentials or anything your agents produce. We don&apos;t store your IP address: it is only used to look up your approximate location (country and city), which is kept with the events.</li>
            <li><strong>Who it&apos;s linked to:</strong> your Codync account when you&apos;re signed in; otherwise a random ID created on that device.</li>
            <li><strong>Why:</strong> to spot bugs sooner and serve you better, by seeing which features work and where people get stuck. It is never sold, never shared for advertising and never used to track you across other apps or websites.</li>
          </ul>
          <p className="mt-2">Codync has no advertising identifiers.</p>
        </Section>

        <Section title="Third-party services">
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>Apple Push Notification service</strong>: delivers notifications and Live Activity updates on iPhone.</li>
            <li><strong>Google Firebase Cloud Messaging</strong>: delivers notifications and live updates on Android.</li>
            <li><strong>Cloudflare Workers</strong>: runs the push relay (nothing stored), the relay between your phone and your computer (computer registrations, messages waiting for an offline computer, webhook deliveries) and the Codync account service; its TURN service forwards a remote screen session&apos;s encrypted video when there&apos;s no direct path.</li>
            <li><strong>Clerk</strong>: runs sign-in for the optional Codync account.</li>
            <li><strong>PostHog (US)</strong>: stores the usage analytics described above, unless you turned them off.</li>
            <li><strong>The AI agents you choose</strong>: they run on your computer under their own terms and privacy policies.</li>
            <li><strong>OpenAI or Google (optional, voice)</strong>: only if you add your own OpenAI or Gemini API key for calls or dictation. In a realtime call, your voice goes from your phone or computer directly to that provider, together with the bot&apos;s name and description and the chat messages it asks for during the call. In a call&apos;s speech mode, and when you dictate a message with the provider, your recording goes to your computer, which sends it to the provider to be transcribed, together with the names of the bots on that computer as spelling hints, and sends the bot&apos;s replies to the provider to be read aloud. All of it is under your account and the provider&apos;s privacy policy. The key is stored encrypted on your computer and sent only to that provider, never to our relay or services; a realtime call gets a short-lived credential made from it. Without a key, voice uses your device&apos;s own speech recognition.</li>
          </ul>
          <p className="mt-2">This policy covers Codync as we publish it. Codync services run by third parties (for example a hosted host or relay) handle your data under their own privacy policies; we don&apos;t operate them and receive none of that data.</p>
        </Section>

        <Section title="Data retention">
          Your bots and conversations live on your devices. Our services keep only these: your account, if you sign in; each computer&apos;s registration, the devices allowed to use it and its routines&apos; webhook keys, while the computer is registered; a log of account and access changes (30 days); messages waiting for an offline computer (up to 24 hours); and webhook deliveries waiting for it (up to 72 hours). Delete a bot to remove its conversation; uninstall the host and delete <code>~/.codync</code> to remove all of it. Deleting the phone app removes its cache. Turning analytics off stops new events from being sent.
        </Section>

        <Section id="delete-account" title="Delete your Codync account">
          <p className="mb-2">
            To delete your Codync account, email{" "}
            <a href={`mailto:${EMAIL}?subject=Delete%20my%20Codync%20account`} className="text-white underline">{EMAIL}</a>{" "}
            from the address you sign in with, with the subject &ldquo;Delete my Codync account&rdquo;. We delete it within 30 days and reply when it&apos;s done.
          </p>
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>Deleted:</strong> your account (email address and account ID), the phones and computers connected to it and their access, and the usage analytics linked to it.</li>
            <li><strong>Kept:</strong> nothing on our side. Bots and conversations stay on your computer until you delete them there (see Data retention).</li>
          </ul>
        </Section>

        <Section title="Children's privacy">
          Codync is not directed at children under the age of 13.
        </Section>

        <Section title="Changes">
          We may update this policy. Changes will be posted on this page with a new date.
        </Section>

        <Section title="Contact">
          Questions? Email{" "}
          <a href={`mailto:${EMAIL}`} className="text-white underline">{EMAIL}</a> or open an issue at{" "}
          <a href={`${GITHUB}/issues`} className="text-white underline">github.com/leepokai/Codync/issues</a>.
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

function Section({ id, title, children }: { id?: string; title: string; children: React.ReactNode }) {
  return (
    <section id={id}>
      <h2 className="text-xl font-semibold text-white mb-2">{title}</h2>
      <div className="text-neutral-400 leading-relaxed">{children}</div>
    </section>
  );
}
