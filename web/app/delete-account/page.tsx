import type { Metadata } from "next";
import Link from "next/link";
import { EMAIL } from "../links";

export const metadata: Metadata = { title: "Delete your account", alternates: { canonical: "/delete-account" } };

export default function DeleteAccount() {
  return (
    <main className="flex-1 flex flex-col items-center px-6 py-16">
      <article className="max-w-2xl w-full space-y-6">
        <h1 className="text-3xl font-bold text-white">Delete your Codync account</h1>

        <Section title="In the app">
          <ul className="list-disc pl-5 space-y-2">
            <li><strong>iPhone and Android:</strong> on the Bots tab, tap the account button at the top left, then <strong>Delete account</strong>.</li>
            <li><strong>Desktop app:</strong> open the account menu and choose <strong>Delete account</strong>.</li>
          </ul>
        </Section>

        <Section title="Without the app">
          Email{" "}
          <a href={`mailto:${EMAIL}?subject=Delete%20my%20Codync%20account`} className="text-white underline">{EMAIL}</a>{" "}
          from the address you sign in with, and ask us to delete your account. We delete it, and the usage analytics linked to it, within 30 days and reply when it&apos;s done.
        </Section>

        <Section title="What is deleted">
          Your sign-in and email address, your account&apos;s list of computers, and every device&apos;s access through the account. Your computers keep their bots and chats, which never leave them. We keep a record that the account was deleted, with the revoked devices&apos; names and keys, so those devices can&apos;t be used with it again. Deleting in the app leaves the usage analytics linked to the account; email us to have them deleted too. See the{" "}
          <Link href="/privacy" className="text-white underline">Privacy Policy</Link>.
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
