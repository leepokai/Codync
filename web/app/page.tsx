import type { Metadata } from "next";
import Hero from "./components/hero";
import BringYourOwn from "./components/bring-your-own";
import Demo from "./components/demo";
import Features from "./components/features";
import Away from "./components/away";
import Privacy from "./components/privacy";
import Compare from "./components/compare";
import Questions from "./components/questions";
import Install from "./components/install";
import { SiteFooter, SiteHeader } from "./components/site-chrome";

export const metadata: Metadata = { alternates: { canonical: "/" } };

export default function Home() {
  return (
    <>
      <SiteHeader />
      <main className="flex-1">
        <Hero />
        <Demo />
        <BringYourOwn />
        <Features />
        <Away />
        <Privacy />
        <Compare />
        <Questions />
        <Install />
      </main>
      <SiteFooter />
    </>
  );
}
