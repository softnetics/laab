import type { CSSProperties, ReactNode } from "react";
import { EyeIcon } from "./Icons";

// One half of the device and the cover screen are the same size, as on the
// iPhone Duo (466 × 678 pt). Drawn in fixed colours: it is a picture of a
// device, not part of LAAB's own theme.
const W = 92;
const H = 134;
const R = 15;

/** The iPhone Duo seen a little from the side. It starts closed on its cover
 *  screen, whose hinge is on the left; the front half swings open to the left
 *  like a book cover, the inner screen lights up with the chat, and it shuts
 *  again. With reduced motion it stays half open. */
export function DuoHint() {
  return (
    <div className="relative mx-auto h-[168px] w-[230px] [perspective:760px]" aria-hidden>
      <div
        className="absolute inset-0 [transform-style:preserve-3d] motion-safe:animate-duo-turn"
        style={{ transform: "translateX(-34px) rotateX(14deg) rotateY(-10deg)" }}
      >
        <div
          className="absolute origin-right rounded-[50%] bg-black/25 blur-[10px] motion-safe:animate-duo-shadow"
          style={{ left: 40, top: 22 + H, width: 2 * W + 20, height: 14 }}
        />
        {/* The back half: the inner screen's right side. */}
        <div className="absolute" style={{ left: 115, top: 14, width: W, height: H }}>
          <Frame corners="right">
            <InnerRight />
          </Frame>
        </div>
        {/* The front half, hinged on its right edge (the spine). Its outer
            face is the cover screen. */}
        <div
          className="absolute origin-right [transform-style:preserve-3d] motion-safe:animate-duo-fold"
          style={{ left: 115 - W, top: 14, width: W, height: H, transform: "rotateY(72deg)" }}
        >
          <div className="absolute inset-0 [backface-visibility:hidden]">
            <Frame corners="left">
              <InnerLeft />
            </Frame>
          </div>
          <div className="absolute inset-0 [backface-visibility:hidden] [transform:rotateY(180deg)_translateZ(0.5px)]">
            <Frame corners="right" outer>
              <Cover />
            </Frame>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Graphite frame, black bezel, glass. `corners` are the rounded ones; the
 *  hinge side stays square. */
function Frame({ corners, outer = false, children }: { corners: "left" | "right"; outer?: boolean; children: ReactNode }) {
  const round = (r: number): CSSProperties =>
    corners === "left"
      ? { borderRadius: `${r}px 3px 3px ${r}px` }
      : { borderRadius: `3px ${r}px ${r}px 3px` };
  return (
    <div
      className="h-full w-full p-[2.5px] shadow-[inset_0_0_0_0.5px_rgba(255,255,255,0.35)]"
      style={{
        ...round(R),
        background: outer
          ? "linear-gradient(135deg, #6b6b70 0%, #2b2b2e 45%, #1a1a1c 100%)"
          : "linear-gradient(135deg, #55555a 0%, #232325 60%, #161618 100%)",
      }}
    >
      <div className="h-full w-full bg-black p-[2px]" style={round(R - 2.5)}>
        <div className="relative h-full w-full overflow-hidden bg-white" style={round(R - 4.5)}>
          {children}
        </div>
      </div>
    </div>
  );
}

const bar = (w: number, color = "#d9d9de") => <span className="block h-[3px] rounded-full" style={{ width: w, background: color }} />;

/** LAAB's chat list on the cover screen, the picked chat marked. */
function Cover() {
  const rows = ["#F4C430", "#5B8DEF", "#E6A8C8", "#7CC4A4", "#A7D08C"];
  return (
    <div className="h-full w-full pt-3 pr-[13px] pl-2">
      <span className="absolute top-[5px] right-[5px] h-[6px] w-[6px] rounded-full bg-black" />
      <div className="mb-2 h-[5px] w-6 rounded-full bg-[#1c1c1e]" />
      <div className="mb-2 h-[7px] w-full rounded-[3px] bg-[#efeff2]" />
      {rows.map((c, i) => (
        <div key={c} className={`mb-[5px] flex items-center gap-1.5 rounded-[3px] ${i === 0 ? "bg-[#06C755]/10" : ""}`}>
          <span className="h-[13px] w-[13px] shrink-0 rounded-full" style={{ background: c }} />
          <div className="flex flex-col gap-[3px]">
            <div className="flex items-center gap-[2px]">
              {i === 0 && <EyeIcon size={6} strokeWidth={3} className="text-[#06C755]" />}
              {bar(i === 0 ? 18 : 22, "#3a3a3c")}
            </div>
            {bar(32)}
          </div>
        </div>
      ))}
      {/* The vertical bar's glass capsule at the trailing edge. */}
      <span className="absolute right-[3px] bottom-3 h-10 w-[7px] rounded-full bg-[#ececf0]" />
    </div>
  );
}

/** Dark until the device opens, as the inner screen turns on. */
function Lights() {
  return <span className="absolute inset-0 bg-black opacity-0 motion-safe:animate-duo-light" />;
}

/** Left of the fold: who the chat is with, and that it is only a peek. */
function InnerLeft() {
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-1.5">
      <span className="h-6 w-6 rounded-full bg-[#F4C430]" />
      {bar(30, "#3a3a3c")}
      <span className="flex items-center gap-[3px]">
        <EyeIcon size={7} strokeWidth={3} className="text-[#06C755]" />
        {bar(24)}
      </span>
      <Lights />
    </div>
  );
}

/** Right of the fold: the messages. */
function InnerRight() {
  const bubble = (w: number, mine = false) => (
    <span
      className={`block h-[9px] rounded-[5px] ${mine ? "self-end" : ""}`}
      style={{ width: w, background: mine ? "#8de08f" : "#ececf0" }}
    />
  );
  return (
    <div className="flex h-full w-full flex-col gap-[5px] px-2 pt-4">
      {bubble(46)}
      {bubble(32)}
      {bubble(40, true)}
      {bubble(54)}
      {bubble(26, true)}
      {bubble(38)}
      <Lights />
    </div>
  );
}
