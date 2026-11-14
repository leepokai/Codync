import Reveal from "./reveal";

// How every homepage section opens: a short label, the heading, and one line under it, centered.
export default function SectionHeader({ label, title, children }: { label?: string; title: string; children?: React.ReactNode }) {
  return (
    <Reveal className="mx-auto max-w-[46rem] text-center">
      {label ? <p className="mb-4 text-sm font-medium text-neutral-500">{label}</p> : null}
      <h2 className="text-4xl font-semibold tracking-tighter text-balance text-neutral-50 md:text-6xl">{title}</h2>
      {children ? <p className="mx-auto mt-6 max-w-[36rem] text-lg leading-relaxed text-balance text-neutral-400">{children}</p> : null}
    </Reveal>
  );
}
