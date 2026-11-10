// A homepage card: its title and a line or two, then a piece of the real app in a well below.
// `id` makes it a link target (its scroll margin also covers the 24px its reveal still has to rise); `wellClassName` sets the well's fill and padding.
export default function Card({
  id,
  title,
  body,
  wellClassName = "bg-neutral-950 p-4",
  children,
}: {
  id?: string;
  title: string;
  body: string;
  wellClassName?: string;
  children: React.ReactNode;
}) {
  return (
    <article id={id} className="flex h-full scroll-mt-28 flex-col rounded-3xl bg-neutral-900 p-2">
      <div className="px-5 pt-5 pb-6 md:px-6 md:pt-6">
        <h3 className="text-lg font-semibold text-neutral-50">{title}</h3>
        <p className="mt-2 leading-relaxed text-neutral-400">{body}</p>
      </div>
      <div className={`mt-auto overflow-hidden rounded-[1.25rem] ${wellClassName}`}>{children}</div>
    </article>
  );
}
