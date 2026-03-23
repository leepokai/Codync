"use client";

import { useRef, useState, useCallback } from "react";
import { motion, useMotionValue, useSpring, useTransform } from "framer-motion";

export default function DeviceShowcase() {
  return (
    <section className="relative px-6 py-16">
      <div className="max-w-5xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-5 gap-6 items-center">
          {/* iPhone — left */}
          <Device
            label="iPhone Live Activity"
            delay={0.15}
            className="md:col-span-1"
            tiltIntensity={20}
            glowId="iphone"
          >
            <div className="relative mx-auto w-[180px] aspect-[9/19.5] rounded-[2rem] border-2 border-neutral-700 bg-neutral-950 overflow-hidden">
              <img
                src="/demo-iphone.png"
                alt="Codync iPhone"
                className="w-full h-full object-cover"
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
              <Placeholder label="iPhone" />
              <DeviceGlow />
            </div>
          </Device>

          {/* Mac — center, largest */}
          <Device
            label="macOS Menu Bar"
            delay={0}
            className="md:col-span-3"
            tiltIntensity={12}
            glowId="mac"
          >
            <div className="relative w-full aspect-[16/10] rounded-xl border border-neutral-800 bg-neutral-950 overflow-hidden">
              <img
                src="/demo-mac.png"
                alt="Codync macOS"
                className="w-full h-full object-cover object-top"
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
              <Placeholder label="macOS" />
              <DeviceGlow />
            </div>
          </Device>

          {/* Watch — right */}
          <Device
            label="Apple Watch"
            delay={0.3}
            className="md:col-span-1"
            tiltIntensity={25}
            glowId="watch"
          >
            <div className="relative mx-auto w-[150px] aspect-square rounded-[2.5rem] border-2 border-neutral-700 bg-neutral-950 overflow-hidden">
              <img
                src="/demo-watch.png"
                alt="Codync Watch"
                className="w-full h-full object-cover"
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
              <Placeholder label="Watch" />
              <DeviceGlow />
            </div>
          </Device>
        </div>
      </div>
    </section>
  );
}

/* ── Glow context: lets Device inject glow inside children's overflow-hidden ── */
import { createContext, useContext } from "react";

type MotionString = ReturnType<typeof useMotionValue<string>>;

const GlowContext = createContext<{
  glowBg: MotionString;
  hovering: boolean;
} | null>(null);

function DeviceGlow() {
  const ctx = useContext(GlowContext);
  if (!ctx) return null;
  return (
    <motion.div
      className="pointer-events-none absolute inset-0 z-10"
      style={{
        opacity: ctx.hovering ? 1 : 0,
        background: ctx.glowBg,
        transition: "opacity 0.3s",
      }}
    />
  );
}

/* ── 3D tilt device wrapper ── */
function Device({
  label,
  delay,
  className,
  children,
  tiltIntensity = 15,
  glowId: _,
}: {
  label: string;
  delay: number;
  className?: string;
  children: React.ReactNode;
  tiltIntensity?: number;
  glowId?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [hovering, setHovering] = useState(false);

  const mouseX = useMotionValue(0);
  const mouseY = useMotionValue(0);

  const rotateX = useSpring(useTransform(mouseY, [-0.5, 0.5], [tiltIntensity, -tiltIntensity]), {
    stiffness: 200,
    damping: 20,
  });
  const rotateY = useSpring(useTransform(mouseX, [-0.5, 0.5], [-tiltIntensity, tiltIntensity]), {
    stiffness: 200,
    damping: 20,
  });

  const glowX = useMotionValue(50);
  const glowY = useMotionValue(50);

  const handleMouse = useCallback(
    (e: React.MouseEvent) => {
      const el = ref.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const x = (e.clientX - rect.left) / rect.width - 0.5;
      const y = (e.clientY - rect.top) / rect.height - 0.5;
      mouseX.set(x);
      mouseY.set(y);
      glowX.set((x + 0.5) * 100);
      glowY.set((y + 0.5) * 100);
    },
    [mouseX, mouseY, glowX, glowY]
  );

  const handleLeave = useCallback(() => {
    setHovering(false);
    mouseX.set(0);
    mouseY.set(0);
    glowX.set(50);
    glowY.set(50);
  }, [mouseX, mouseY, glowX, glowY]);

  const glowBg = useTransform(
    [glowX, glowY],
    ([x, y]) =>
      `radial-gradient(circle at ${x}% ${y}%, rgba(255,255,255,0.1) 0%, transparent 60%)`
  );

  return (
    <motion.div
      initial={{ opacity: 0, y: 40 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-50px" }}
      transition={{ duration: 0.7, delay, ease: [0.21, 0.47, 0.32, 0.98] }}
      className={`flex flex-col items-center gap-4 ${className}`}
      style={{ perspective: 800 }}
    >
      <motion.div
        ref={ref}
        onMouseMove={(e) => {
          setHovering(true);
          handleMouse(e);
        }}
        onMouseLeave={handleLeave}
        style={{
          rotateX,
          rotateY,
          transformStyle: "preserve-3d",
        }}
        className="relative"
      >
        <GlowContext.Provider value={{ glowBg, hovering }}>
          {children}
        </GlowContext.Provider>
      </motion.div>

      <p className="text-sm text-neutral-500 font-medium">{label}</p>
    </motion.div>
  );
}

function Placeholder({ label }: { label: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center text-neutral-700 text-sm pointer-events-none">
      {label}
    </div>
  );
}
