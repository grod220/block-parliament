import { useState, useEffect } from "react";

const SHADES = ["▒", "░"];
const SEGMENT = " - - - "; // 3 dashes with spaces

function getRandomShade() {
  return SHADES[Math.floor(Math.random() * SHADES.length)];
}

function generateInitialLine(length: number): string {
  let line = "";
  while (line.length < length) {
    line += getRandomShade() + SEGMENT;
  }
  return line;
}

function AnimatedLine() {
  const [line, setLine] = useState(() => generateInitialLine(50));

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const interval = setInterval(() => {
      setLine((prev) => {
        const shifted = prev.slice(1);
        if (shifted.length < 50) {
          return shifted + getRandomShade() + SEGMENT;
        }
        return shifted;
      });
    }, 400);
    return () => clearInterval(interval);
  }, []);

  return <span className="text-[var(--ink-light)]">{line.slice(0, 20)}</span>;
}

export function AnimatedGradientDashBorder({ title }: { title: string }) {
  return (
    <div className="select-none overflow-hidden whitespace-nowrap flex justify-center items-center">
      <AnimatedLine />
      <span className="font-bold px-6">{title}</span>
      <AnimatedLine />
    </div>
  );
}
