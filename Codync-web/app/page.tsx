import WaitlistForm from "./waitlist-form";

export default function Home() {
  return (
    <main className="flex-1 flex flex-col items-center justify-center px-6">
      <div className="max-w-2xl w-full text-center space-y-8 py-24">
        <div className="flex justify-center">
          <img
            src="/icon.png"
            alt="Codync"
            className="w-20 h-20 rounded-2xl"
          />
        </div>

        <div className="space-y-3">
          <h1 className="text-4xl font-bold tracking-tight text-white">
            Codync
          </h1>
          <p className="text-lg text-neutral-400">
            Real-time Claude Code monitor for iPhone and Mac
          </p>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 text-left">
          <Feature
            title="Dynamic Island"
            description="Live session status on your Lock Screen and Dynamic Island"
          />
          <Feature
            title="Menu Bar"
            description="macOS menu bar app with instant session overview"
          />
          <Feature
            title="Always-on Push"
            description="Live Activity stays updated even when the app is closed"
          />
        </div>

        {/* Waitlist */}
        <WaitlistForm />

        <div className="flex flex-col items-center gap-3 pt-4">
          <a
            href="https://apps.apple.com/app/id6760984418"
            className="inline-flex items-center gap-2 px-6 py-3 bg-white text-black font-semibold rounded-xl hover:bg-neutral-200 transition-colors"
          >
            Download on the App Store
          </a>
          <p className="text-sm text-neutral-500">
            Free with optional Pro subscription
          </p>
        </div>

        <div className="flex justify-center gap-6 pt-8 text-sm text-neutral-500">
          <a href="/privacy" className="hover:text-neutral-300 transition-colors">
            Privacy Policy
          </a>
          <a href="mailto:kevin2005ha@gmail.com" className="hover:text-neutral-300 transition-colors">
            Contact
          </a>
          <a href="https://github.com/leepokai/CodePulse" className="hover:text-neutral-300 transition-colors">
            GitHub
          </a>
        </div>
      </div>
    </main>
  );
}

function Feature({ title, description }: { title: string; description: string }) {
  return (
    <div className="p-4 rounded-xl border border-neutral-800 bg-neutral-900/50">
      <h3 className="font-medium text-white text-sm">{title}</h3>
      <p className="text-sm text-neutral-400 mt-1">{description}</p>
    </div>
  );
}
