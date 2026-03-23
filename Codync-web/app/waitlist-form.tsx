"use client";

import { useState } from "react";

const GOOGLE_FORM_URL =
  "https://docs.google.com/forms/d/e/1FAIpQLSdPS0rfrsG1GGsdbSXFJbmRo5d9j14Na4z--TWsB5iexynWmQ/formResponse";
const EMAIL_ENTRY = "entry.2002260518";

export default function WaitlistForm() {
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<"idle" | "loading" | "success" | "error">("idle");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!email) return;
    setStatus("loading");

    try {
      const body = new URLSearchParams({ [EMAIL_ENTRY]: email });
      await fetch(GOOGLE_FORM_URL, {
        method: "POST",
        mode: "no-cors",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: body.toString(),
      });
      // Google Forms returns opaque response with no-cors, assume success
      setStatus("success");
      setEmail("");
    } catch {
      setStatus("error");
    }
  }

  if (status === "success") {
    return (
      <div className="flex flex-col items-center gap-2 py-2">
        <p className="text-sm text-green-400">You&apos;re on the list!</p>
        <p className="text-xs text-neutral-500">We&apos;ll notify you when Codync launches.</p>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col items-center gap-3">
      <p className="text-sm text-neutral-400">Join the waitlist for early access</p>
      <div className="flex gap-2 w-full max-w-sm">
        <input
          type="email"
          required
          placeholder="your@email.com"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          className="flex-1 px-4 py-2.5 rounded-xl bg-neutral-900 border border-neutral-800 text-white placeholder-neutral-500 text-sm focus:outline-none focus:border-neutral-600 transition-colors"
        />
        <button
          type="submit"
          disabled={status === "loading"}
          className="px-5 py-2.5 bg-white text-black font-medium text-sm rounded-xl hover:bg-neutral-200 transition-colors disabled:opacity-50"
        >
          {status === "loading" ? "..." : "Join"}
        </button>
      </div>
      {status === "error" && (
        <p className="text-xs text-red-400">Something went wrong. Try again.</p>
      )}
    </form>
  );
}
