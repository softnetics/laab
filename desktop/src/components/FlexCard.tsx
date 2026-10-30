import type { CSSProperties, ReactNode } from "react";

// LINE flex messages drawn as cards. Cards are always light, LINE px are
// CSS px 1:1. Unsupported: `%` sizes and `position: absolute`.

type Node = Record<string, any>;
export type FlexAction = {
  type?: string;
  label?: string;
  uri?: string;
  altUri?: { desktop?: string };
  data?: string;
  text?: string;
  displayText?: string;
  clipboardText?: string;
};

const SPACE: Record<string, number> = { none: 0, xs: 2, sm: 4, md: 8, lg: 12, xl: 16, xxl: 20 };
const BORDER: Record<string, number> = { none: 0, light: 0.5, normal: 1, medium: 2, "semi-bold": 3, bold: 4 };
const TEXT: Record<string, number> = { xxs: 11, xs: 13, sm: 14, md: 16, lg: 19, xl: 22, xxl: 29, "3xl": 35, "4xl": 48, "5xl": 74 };
const IMAGE: Record<string, number> = { xxs: 40, xs: 60, sm: 80, md: 100, lg: 120, xl: 140, xxl: 160, "3xl": 180, "4xl": 200, "5xl": 220 };
const WIDTH: Record<string, number> = { nano: 120, micro: 160, deca: 220, hecto: 241, kilo: 260, mega: 300, giga: 386 };

const px = (v: unknown, table = SPACE): number | undefined => {
  if (typeof v !== "string") return undefined;
  if (v in table) return table[v];
  const m = /^(\d+(?:\.\d+)?)px$/.exec(v);
  return m ? Number(m[1]) : undefined;
};

export function parseFlex(json: string | undefined): Node | null {
  if (!json) return null;
  try {
    const root = JSON.parse(json);
    return root?.type === "bubble" || root?.type === "carousel" ? root : null;
  } catch {
    return null;
  }
}

export function FlexCard({ root, onAction }: { root: Node; onAction: (a: FlexAction) => void }) {
  if (root.type === "carousel") {
    return (
      <div className="flex max-w-[560px] gap-2 overflow-x-auto pb-1">
        {(root.contents ?? []).map((b: Node, i: number) => (
          <Bubble key={i} node={b} onAction={onAction} />
        ))}
      </div>
    );
  }
  return <Bubble node={root} onAction={onAction} />;
}

function Bubble({ node, onAction }: { node: Node; onAction: (a: FlexAction) => void }) {
  const width = WIDTH[node.size] ?? 300;
  const styles = node.styles ?? {};
  const block = (key: "header" | "hero" | "body" | "footer", pad: number) => {
    const n = node[key];
    if (!n) return null;
    const st = styles[key] ?? {};
    return (
      <div
        key={key}
        style={{
          background: st.backgroundColor,
          borderTop: st.separator ? `1px solid ${st.separatorColor ?? "#E5E5E5"}` : undefined,
        }}
      >
        {/* A section's padding is its root box's default, not extra space. */}
        <Component node={n} onAction={onAction} defaultPad={pad} />
      </div>
    );
  };
  return (
    <div
      className="shrink-0 overflow-hidden rounded-2xl bg-white text-[#111111] shadow-[0_1px_2px_rgba(0,0,0,0.12)]"
      style={{ width, colorScheme: "light", direction: node.direction === "rtl" ? "rtl" : undefined }}
      onClick={(e) => {
        if (node.action && e.target === e.currentTarget) onAction(node.action);
      }}
    >
      {block("header", 20)}
      {block("hero", 0)}
      {block("body", 20)}
      {block("footer", 10)}
    </div>
  );
}

function boxStyle(n: Node, defaultPad?: number): CSSProperties {
  const pad = px(n.paddingAll) ?? defaultPad;
  return {
    background: n.backgroundColor,
    borderColor: n.borderColor,
    borderWidth: px(n.borderWidth, BORDER),
    borderStyle: n.borderWidth ? "solid" : undefined,
    borderRadius: px(n.cornerRadius),
    paddingTop: px(n.paddingTop) ?? pad,
    paddingBottom: px(n.paddingBottom) ?? pad,
    paddingLeft: px(n.paddingStart) ?? pad,
    paddingRight: px(n.paddingEnd) ?? pad,
    width: px(n.width),
    height: px(n.height),
    justifyContent: n.justifyContent,
    alignItems: n.alignItems,
  };
}

function flexOf(n: Node, horizontal: boolean): CSSProperties {
  const noFlex = ["icon", "spacer", "separator"].includes(n.type);
  const f = typeof n.flex === "number" ? n.flex : noFlex ? 0 : 1;
  if (!horizontal) return f > 0 && n.type === "filler" ? { flex: 1 } : {};
  return f <= 0 ? { flex: "0 0 auto" } : { flex: `${f} 1 0%`, minWidth: 0 };
}

function Component({
  node: n,
  onAction,
  horizontal = false,
  gap,
  defaultPad,
}: {
  node: Node;
  onAction: (a: FlexAction) => void;
  horizontal?: boolean;
  gap?: number;
  defaultPad?: number;
}) {
  const margin = px(n.margin) ?? gap;
  const absolute = n.position === "absolute";
  const offsets: CSSProperties = {
    top: px(n.offsetTop),
    left: px(n.offsetStart),
    right: px(n.offsetEnd),
    bottom: px(n.offsetBottom),
  };
  const outer: CSSProperties = absolute
    ? // Drawn over its siblings, sized to its content.
      { position: "absolute", zIndex: 1, ...offsets }
    : {
        ...flexOf(n, horizontal),
        [horizontal ? "marginLeft" : "marginTop"]: margin,
        alignSelf: n.gravity === "center" ? "center" : n.gravity === "bottom" ? "flex-end" : undefined,
        ...(Object.values(offsets).some((v) => v != null) ? { position: "relative", ...offsets } : {}),
      };
  const clickable = n.action
    ? { onClick: (e: React.MouseEvent) => (e.stopPropagation(), onAction(n.action)), style: { cursor: "pointer" } }
    : {};
  switch (n.type) {
    case "box": {
      const layout = n.layout ?? "vertical";
      const h = layout !== "vertical";
      const spacing = px(n.spacing) ?? 0;
      return (
        <div
          {...clickable}
          style={{
            position: "relative",
            ...outer,
            ...boxStyle(n, defaultPad),
            display: "flex",
            flexDirection: h ? "row" : "column",
            alignItems: layout === "baseline" ? "baseline" : n.alignItems,
            ...(clickable.style ?? {}),
          }}
        >
          {(n.contents ?? []).map((c: Node, i: number) => (
            <Component key={i} node={c} onAction={onAction} horizontal={h} gap={i === 0 ? undefined : spacing} />
          ))}
        </div>
      );
    }
    case "text": {
      const size = px(n.size, TEXT) ?? 16;
      const wrap = n.wrap === true;
      const style: CSSProperties = {
        ...outer,
        fontSize: size,
        lineHeight: n.lineSpacing ? `${size + (px(n.lineSpacing) ?? 0)}px` : 1.3,
        color: n.color ?? "#111111",
        fontWeight: n.weight === "bold" ? 700 : 400,
        fontStyle: n.style === "italic" ? "italic" : undefined,
        textDecoration: n.decoration === "underline" ? "underline" : n.decoration === "line-through" ? "line-through" : undefined,
        textAlign: n.align === "end" ? "end" : n.align === "center" ? "center" : undefined,
        whiteSpace: wrap ? "pre-wrap" : "nowrap",
        overflow: "hidden",
        textOverflow: wrap ? undefined : "ellipsis",
        display: wrap && n.maxLines ? "-webkit-box" : undefined,
        WebkitLineClamp: wrap && n.maxLines ? n.maxLines : undefined,
        WebkitBoxOrient: wrap && n.maxLines ? "vertical" : undefined,
        ...(clickable.style ?? {}),
      };
      return (
        <div onClick={clickable.onClick} style={style}>
          {n.contents?.length ? n.contents.map((s: Node, i: number) => <Span key={i} node={s} />) : n.text}
        </div>
      );
    }
    case "image": {
      const full = n.size === "full";
      const w = full ? "100%" : (px(n.size, IMAGE) ?? 100);
      const [rw, rh] = String(n.aspectRatio ?? "1:1")
        .split(":")
        .map(Number);
      return (
        <div
          onClick={clickable.onClick}
          style={{
            ...outer,
            width: full ? "100%" : undefined,
            display: "flex",
            justifyContent: n.align === "start" ? "flex-start" : n.align === "end" ? "flex-end" : "center",
            ...(clickable.style ?? {}),
          }}
        >
          <img
            src={n.url}
            alt=""
            draggable={false}
            style={{
              width: w,
              aspectRatio: `${rw || 1} / ${rh || 1}`,
              objectFit: n.aspectMode === "cover" ? "cover" : "contain",
              background: n.backgroundColor ?? "#EDEEF0",
            }}
          />
        </div>
      );
    }
    case "button": {
      const style = n.style ?? "link";
      const colors =
        style === "primary"
          ? { background: n.color ?? "#17C950", color: "#FFFFFF" }
          : style === "secondary"
            ? { background: n.color ?? "#DCDFE5", color: "#111111" }
            : { background: "transparent", color: n.color ?? "#42659A" };
      return (
        <button
          onClick={(e) => (e.stopPropagation(), n.action && onAction(n.action))}
          className="w-full rounded-lg px-2 text-[16px] font-medium active:opacity-70"
          style={{ ...outer, ...colors, height: n.height === "sm" ? 40 : 52 }}
        >
          <span className="line-clamp-1">{n.action?.label ?? ""}</span>
        </button>
      );
    }
    case "icon": {
      const size = px(n.size, TEXT) ?? 16;
      return <img src={n.url} alt="" style={{ ...outer, width: size, height: size, objectFit: "contain" }} />;
    }
    case "separator":
      return (
        <div
          style={{
            ...outer,
            alignSelf: "stretch",
            background: n.color ?? "#E5E5E5",
            [horizontal ? "width" : "height"]: 1,
            flex: "0 0 auto",
          }}
        />
      );
    case "spacer":
      return <div style={{ ...outer, flex: "0 0 auto", [horizontal ? "width" : "height"]: px(n.size) ?? 8 }} />;
    case "filler":
      return <div style={{ flex: n.flex ?? 1 }} />;
    case "video":
      return n.altContent ? <Component node={n.altContent} onAction={onAction} horizontal={horizontal} /> : null;
    default:
      return null;
  }
}

function Span({ node }: { node: Node }): ReactNode {
  return (
    <span
      style={{
        color: node.color,
        fontSize: px(node.size, TEXT),
        fontWeight: node.weight === "bold" ? 700 : undefined,
        fontStyle: node.style === "italic" ? "italic" : undefined,
        textDecoration: node.decoration === "underline" ? "underline" : node.decoration === "line-through" ? "line-through" : undefined,
      }}
    >
      {node.text}
    </span>
  );
}
