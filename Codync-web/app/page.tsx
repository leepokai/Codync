import WaitlistForm from "./waitlist-form";
import Hero from "./components/hero";
import DeviceShowcase from "./components/device-showcase";
import Features from "./components/features";

export default function Home() {
  return (
    <main className="flex-1 flex flex-col">
      {/* Hero */}
      <Hero />

      {/* Device Showcase */}
      <DeviceShowcase />

      {/* Features */}
      <Features />

      {/* Waitlist + CTA */}
      <section className="flex flex-col items-center gap-8 px-6 py-20">
        <WaitlistForm />

        <div className="flex flex-col items-center gap-3">
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
      </section>

      {/* Footer */}
      <footer className="flex justify-center gap-6 px-6 py-10 text-sm text-neutral-500 border-t border-neutral-900">
        <a href="/privacy" className="hover:text-neutral-300 transition-colors">
          Privacy Policy
        </a>
        <a href="mailto:kevin2005ha@gmail.com" className="hover:text-neutral-300 transition-colors">
          Contact
        </a>
        <a href="https://github.com/leepokai/CodePulse" className="hover:text-neutral-300 transition-colors">
          GitHub
        </a>
      </footer>
    </main>
  );
}
