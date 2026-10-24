import type { Metadata } from "next";
import Hero from "./components/hero";
import Film from "./components/film";
import { Answers, Features } from "./components/features";
import Fallbacks from "./components/fallbacks";
import Computers from "./components/computers";
import Surfaces from "./components/surfaces";
import Privacy from "./components/privacy";
import Compare from "./components/compare";
import Cost from "./components/cost";
import Questions from "./components/questions";
import CompareCards from "./components/compare-cards";
import Install from "./components/install";
import Reveal from "./components/reveal";
import { SiteFooter, SiteHeader } from "./components/site-chrome";

export const metadata: Metadata = { alternates: { canonical: "/" } };

export default function Home() {
  return (
    <>
      <SiteHeader />
      <main className="flex-1">
        <Hero />
        <Film />
        <Answers />
        <Features />
        <Fallbacks />
        <Computers />
        <Surfaces />
        <Privacy />
        <Compare />
        <Cost />
        <Questions />
        <section className="px-4 pb-20 sm:px-6 md:pb-28">
          <Reveal className="mx-auto max-w-6xl">
            <h2 className="mb-6 text-xl font-semibold text-neutral-50">Compare Codync</h2>
            <CompareCards />
          </Reveal>
        </section>
        <Install />
      </main>
      <SiteFooter />
    </>
  );
}
