"use client";

import { useRef, useState, useCallback, createContext, useContext } from "react";
import { motion, useMotionValue, useSpring, useTransform } from "framer-motion";

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

export default function DeviceShowcase() {
  return (
    <section className="relative px-6 py-16 overflow-visible">
      <div className="max-w-5xl mx-auto">
        <div className="relative w-full">

          {/* Mac — large background, takes full width */}
          <motion.div
            initial={{ opacity: 0, y: 30 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.7, ease: [0.21, 0.47, 0.32, 0.98] }}
          >
            <p className="text-xs text-neutral-500 font-medium mb-2 ml-1">macOS</p>
            <div className="relative w-full aspect-[16/10] rounded-2xl border border-neutral-800 bg-neutral-950 overflow-hidden">
              <img
                src="/demo-mac.png"
                alt="Codync macOS"
                className="w-full h-full object-cover object-top"
                onError={(e) => { e.currentTarget.style.display = "none"; }}
              />
            </div>
          </motion.div>

          {/* iPhone — bottom-left, inside Mac */}
          <div className="absolute left-[4%] md:left-[6%] bottom-[4%] z-10">
            <TiltCard delay={0.15} tiltIntensity={20}>
              <p className="text-xs text-neutral-400 font-medium mb-1.5 ml-1">iPhone</p>
              <div className="relative w-[140px] md:w-[180px] aspect-[9/19.5] rounded-[1.8rem] md:rounded-[2.2rem] border-2 border-neutral-600 bg-neutral-950 overflow-hidden shadow-[0_10px_60px_rgba(0,0,0,0.8)]">
                <img
                  src="/demo-iphone.png"
                  alt="Codync iPhone"
                  className="w-full h-full object-cover"
                  onError={(e) => { e.currentTarget.style.display = "none"; }}
                />
                <DeviceGlow />
              </div>
            </TiltCard>
          </div>

          {/* Watch — bottom-right, inside Mac */}
          <div className="absolute right-[4%] md:right-[7%] bottom-[6%] z-10">
            <TiltCard delay={0.3} tiltIntensity={25}>
              <p className="text-xs text-neutral-400 font-medium mb-1.5 ml-1">watchOS</p>
              <div className="relative w-[120px] md:w-[170px] aspect-square rounded-[1.8rem] md:rounded-[2.2rem] border-2 border-neutral-600 bg-neutral-950 overflow-hidden shadow-[0_10px_60px_rgba(0,0,0,0.8)]">
                <img
                  src="/demo-watch.png"
                  alt="Codync Watch"
                  className="w-full h-full object-cover"
                  onError={(e) => { e.currentTarget.style.display = "none"; }}
                />
                <DeviceGlow />
              </div>
            </TiltCard>
          </div>

        </div>


      </div>
    </section>
  );
}

/* ── Tilt card with glow (for iPhone & Watch) ── */
function TiltCard({
  delay,
  children,
  tiltIntensity = 15,
}: {
  delay: number;
  children: React.ReactNode;
  tiltIntensity?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [hovering, setHovering] = useState(false);

  const mouseX = useMotionValue(0);
  const mouseY = useMotionValue(0);

  const rotateX = useSpring(useTransform(mouseY, [-0.5, 0.5], [tiltIntensity, -tiltIntensity]), {
    stiffness: 200, damping: 20,
  });
  const rotateY = useSpring(useTransform(mouseX, [-0.5, 0.5], [-tiltIntensity, tiltIntensity]), {
    stiffness: 200, damping: 20,
  });

  const glowX = useMotionValue(50);
  const glowY = useMotionValue(50);

  const handleMouse = useCallback(
    (e: React.MouseEvent) => {
      const el = ref.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      mouseX.set((e.clientX - rect.left) / rect.width - 0.5);
      mouseY.set((e.clientY - rect.top) / rect.height - 0.5);
      glowX.set(((e.clientX - rect.left) / rect.width) * 100);
      glowY.set(((e.clientY - rect.top) / rect.height) * 100);
    },
    [mouseX, mouseY, glowX, glowY]
  );

  const handleLeave = useCallback(() => {
    setHovering(false);
    mouseX.set(0); mouseY.set(0);
    glowX.set(50); glowY.set(50);
  }, [mouseX, mouseY, glowX, glowY]);

  const glowBg = useTransform(
    [glowX, glowY],
    ([x, y]) => `radial-gradient(circle at ${x}% ${y}%, rgba(255,255,255,0.12) 0%, transparent 60%)`
  );

  return (
    <motion.div
      initial={{ opacity: 0, y: 30 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true }}
      transition={{ duration: 0.7, delay, ease: [0.21, 0.47, 0.32, 0.98] }}
      style={{ perspective: 800 }}
    >
      <motion.div
        ref={ref}
        onMouseMove={(e) => { setHovering(true); handleMouse(e); }}
        onMouseLeave={handleLeave}
        style={{ rotateX, rotateY, transformStyle: "preserve-3d" }}
      >
        <GlowContext.Provider value={{ glowBg, hovering }}>
          {children}
        </GlowContext.Provider>
      </motion.div>
    </motion.div>
  );
}

