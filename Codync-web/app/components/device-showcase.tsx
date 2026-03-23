"use client";

import { motion } from "framer-motion";

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
          >
            <div className="relative mx-auto w-[180px] aspect-[9/19.5] rounded-[2rem] overflow-hidden border-2 border-neutral-700 bg-neutral-950 shadow-2xl shadow-white/5">
              <img
                src="/demo-iphone.png"
                alt="Codync iPhone"
                className="w-full h-full object-cover"
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
              <Placeholder label="iPhone" />
            </div>
          </Device>

          {/* Mac — center, largest */}
          <Device
            label="macOS Menu Bar"
            delay={0}
            className="md:col-span-3"
          >
            <div className="relative w-full aspect-[16/10] rounded-xl overflow-hidden border border-neutral-800 bg-neutral-950 shadow-2xl shadow-white/5">
              <img
                src="/demo-mac.png"
                alt="Codync macOS"
                className="w-full h-full object-cover object-top"
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
              <Placeholder label="macOS" />
            </div>
          </Device>

          {/* Watch — right */}
          <Device
            label="Apple Watch"
            delay={0.3}
            className="md:col-span-1"
          >
            <div className="relative mx-auto w-[150px] aspect-square rounded-[2.5rem] overflow-hidden border-2 border-neutral-700 bg-neutral-950 shadow-2xl shadow-white/5">
              <img
                src="/demo-watch.png"
                alt="Codync Watch"
                className="w-full h-full object-cover"
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
              <Placeholder label="Watch" />
            </div>
          </Device>
        </div>
      </div>
    </section>
  );
}

function Device({
  label,
  delay,
  className,
  children,
}: {
  label: string;
  delay: number;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 30 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-50px" }}
      transition={{ duration: 0.6, delay }}
      className={`flex flex-col items-center gap-4 ${className}`}
    >
      {children}
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
