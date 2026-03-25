import Hero from "./components/hero";
import DeviceShowcase from "./components/device-showcase";
import VisualDemos from "./components/visual-demos";
import DemoVideo from "./components/demo-video";

export default function Home() {
  return (
    <main className="flex-1 flex flex-col">
      {/* Hero */}
      <Hero />

      {/* Device Showcase */}
      <DeviceShowcase />

      {/* Demo Video */}
      <DemoVideo />

      {/* Visual Feature Demos */}
      <VisualDemos />

      {/* CTA */}
      <section className="flex flex-col items-center gap-8 px-6 py-20">
        <div className="flex flex-col items-center gap-3">
          <span
            className="inline-flex items-center gap-2 px-6 py-3 bg-neutral-700 text-neutral-400 font-semibold rounded-xl cursor-not-allowed"
          >
            Coming Soon on the App Store
          </span>
          <p className="text-sm text-neutral-500">
            Free &amp; open source. No account needed — just download and go.
          </p>
        </div>
      </section>

      {/* Footer */}
      <footer className="flex justify-center gap-6 px-6 py-10 text-sm text-neutral-500 border-t border-neutral-900">
        <a href="/terms" className="hover:text-neutral-300 transition-colors">
          Terms of Use
        </a>
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
