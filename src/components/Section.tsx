import type { ReactNode } from "react";

interface SectionProps {
  id: string;
  title: string;
  children: ReactNode;
}

export function Section({ id, title, children }: SectionProps) {
  return (
    <section id={id} className="mb-8">
      <h2 className="font-bold uppercase mb-3">─┤ {title} ├─</h2>
      <div className="pl-4 border-l border-dashed border-[var(--rule)]">{children}</div>
    </section>
  );
}
