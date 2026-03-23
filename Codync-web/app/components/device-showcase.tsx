"use client";

import { useRef, useState, useCallback } from "react";
import { motion, useMotionValue, useSpring, useTransform } from "framer-motion";

export default function DeviceShowcase() {
  return (
    <section className="relative px-6 py-16">
      <div className="max-w-5xl mx-auto">
        {/* Mac as large background, iPhone & Watch overlaid */}
        <div className="relative inline-block w-full">

          {/* Mac — full width */}
          <Device
            label="macOS Menu Bar"
            delay={0}
            className="w-full"
            tiltIntensity={8}
            glowId="mac"
            hideLabel
          >
            <div className="relative w-full aspect-[16/10] rounded-2xl border border-neutral-800 bg-neutral-950 overflow-hidden">
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

          {/* iPhone — bottom-left overlap */}
          <div className="absolute left-[3%] md:left-[5%] bottom-[-12%] z-10">
            <Device
              label="iPhone"
              delay={0.15}
              className=""
              tiltIntensity={20}
              glowId="iphone"
              hideLabel
            >
              <div className="relative w-[120px] md:w-[160px] aspect-[9/19.5] rounded-[1.8rem] border-2 border-neutral-700 bg-neutral-950 overflow-hidden shadow-[0_8px_40px_rgba(0,0,0,0.6)]">
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
          </div>

          {/* Watch — bottom-right overlap */}
          <div className="absolute right-[5%] md:right-[8%] bottom-[-6%] z-10">
            <Device
              label="Watch"
              delay={0.3}
              className=""
              tiltIntensity={25}
              glowId="watch"
              hideLabel
            >
              <div className="relative w-[90px] md:w-[120px] aspect-square rounded-[2rem] border-2 border-neutral-700 bg-neutral-950 overflow-hidden shadow-[0_8px_40px_rgba(0,0,0,0.6)]">
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

        {/* Labels */}
        <div className="flex justify-center gap-8 mt-20 text-sm text-neutral-500 font-medium">
          <span>iPhone</span>
          <span>macOS</span>
          <span>Apple Watch</span>
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
  hideLabel = false,
}: {
  label: string;
  delay: number;
  className?: string;
  children: React.ReactNode;
  tiltIntensity?: number;
  glowId?: string;
  hideLabel?: boolean;
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

      {!hideLabel && <p className="text-sm text-neutral-500 font-medium">{label}</p>}
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
