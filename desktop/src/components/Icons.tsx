import type { SVGProps } from "react";

type P = SVGProps<SVGSVGElement> & { size?: number };
const base = ({ size = 20, ...p }: P) => ({ width: size, height: size, viewBox: "0 0 24 24", fill: "currentColor", ...p });

export const PersonIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M12 12a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9Zm0 2c-4.4 0-8 2.2-8 5v1.5c0 .3.2.5.5.5h15c.3 0 .5-.2.5-.5V19c0-2.8-3.6-5-8-5Z" />
  </svg>
);
export const BubbleIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M12 3C6.5 3 2 6.6 2 11c0 2.5 1.4 4.7 3.6 6.2L5 21l4-2.2c1 .3 2 .4 3 .4 5.5 0 10-3.6 10-8.1S17.5 3 12 3Z" />
  </svg>
);
export const BubblesIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M9 3C4.6 3 1 5.9 1 9.5c0 2 1.1 3.8 2.9 5L3.5 18l3.3-1.8c.7.2 1.4.3 2.2.3 4.4 0 8-2.9 8-6.5S13.4 3 9 3Zm10.6 6.3c.3.7.4 1.4.4 2.2 0 4.3-4 7.8-9.2 8.4 1.3 1.3 3.3 2.1 5.7 2.1.7 0 1.3-.1 1.9-.2L21 23l-.4-3.1c1.5-1.1 2.4-2.6 2.4-4.4 0-2.6-1.4-4.9-3.4-6.2Z" />
  </svg>
);
export const SearchIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round">
    <circle cx="10.5" cy="10.5" r="6.5" />
    <path d="m20 20-4.6-4.6" />
  </svg>
);
export const FolderPlusIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M3 6.5A2.5 2.5 0 0 1 5.5 4h3.6c.6 0 1.2.3 1.6.7L12 6h6.5A2.5 2.5 0 0 1 21 8.5v9a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5v-11Zm9 4a1 1 0 0 0-1 1V13H9.5a1 1 0 1 0 0 2H11v1.5a1 1 0 1 0 2 0V15h1.5a1 1 0 1 0 0-2H13v-1.5a1 1 0 0 0-1-1Z" />
  </svg>
);
export const FolderIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M3 6.5A2.5 2.5 0 0 1 5.5 4h3.6c.6 0 1.2.3 1.6.7L12 6h6.5A2.5 2.5 0 0 1 21 8.5v9a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5v-11Z" />
  </svg>
);
export const SyncIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M20 11a8 8 0 0 0-14.6-4.5M4 4v3h3M4 13a8 8 0 0 0 14.6 4.5M20 20v-3h-3" />
  </svg>
);
export const MoreIcon = (p: P) => (
  <svg {...base(p)}>
    <circle cx="5" cy="12" r="2" />
    <circle cx="12" cy="12" r="2" />
    <circle cx="19" cy="12" r="2" />
  </svg>
);
export const MoreVIcon = (p: P) => (
  <svg {...base(p)}>
    <circle cx="12" cy="5" r="2" />
    <circle cx="12" cy="12" r="2" />
    <circle cx="12" cy="19" r="2" />
  </svg>
);
export const PinIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M14.8 2.6a1 1 0 0 1 1.4 0l5.2 5.2a1 1 0 0 1 0 1.4l-1.3 1.3a1 1 0 0 1-1.1.2l-.9-.4-3.6 3.6.3 2.6a1 1 0 0 1-.3.8l-1.2 1.2a1 1 0 0 1-1.4 0l-2.8-2.8-4.4 4.4a1 1 0 1 1-1.4-1.4l4.4-4.4-2.8-2.8a1 1 0 0 1 0-1.4l1.2-1.2a1 1 0 0 1 .8-.3l2.6.3 3.6-3.6-.4-.9a1 1 0 0 1 .2-1.1l1.3-1.3Z" />
  </svg>
);
export const BellSlashIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M3.3 2.3a1 1 0 0 0-1.4 1.4l18.4 18.4a1 1 0 0 0 1.4-1.4l-2.4-2.4h.2a1 1 0 0 0 .8-1.6L19 15.2V10a7 7 0 0 0-10.9-5.8L3.3 2.3ZM5.1 8.3A7 7 0 0 0 5 10v5.2l-1.3 1.9a1 1 0 0 0 .8 1.6h10.3L5.1 8.3ZM9.5 20a2.5 2.5 0 0 0 5 0h-5Z" />
  </svg>
);
export const ChevronIcon = ({ dir = "left", ...p }: P & { dir?: "left" | "right" | "up" | "down" }) => {
  const r = { left: 0, up: 90, right: 180, down: 270 }[dir];
  return (
    <svg
      {...base(p)}
      fill="none"
      stroke="currentColor"
      strokeWidth={2.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ transform: `rotate(${r}deg)` }}
    >
      <path d="m15 5-7 7 7 7" />
    </svg>
  );
};
export const CloseIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round">
    <path d="M6 6l12 12M18 6 6 18" />
  </svg>
);
export const PlusBubbleIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M12 3C6.5 3 2 6.6 2 11c0 2.5 1.4 4.7 3.6 6.2L5 21l4-2.2c1 .3 2 .4 3 .4 5.5 0 10-3.6 10-8.1S17.5 3 12 3Zm1 5v2h2a1 1 0 1 1 0 2h-2v2a1 1 0 1 1-2 0v-2H9a1 1 0 1 1 0-2h2V8a1 1 0 1 1 2 0Z" />
  </svg>
);
export const SortIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M7 4v16M3.5 7.5 7 4l3.5 3.5M17 20V4M13.5 16.5 17 20l3.5-3.5" />
  </svg>
);
export const ExternalIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <rect x="3.5" y="3.5" width="17" height="17" rx="4" />
    <path d="M10 14 16 8M11 8h5v5" />
  </svg>
);
export const PaperclipIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={p.strokeWidth ?? 2} strokeLinecap="round">
    <path d="m20 11.5-8.1 8.1a5 5 0 0 1-7.1-7.1l8.5-8.5a3.3 3.3 0 0 1 4.7 4.7l-8.5 8.5a1.7 1.7 0 0 1-2.4-2.4l7.8-7.8" />
  </svg>
);
export const SmileIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={p.strokeWidth ?? 2} strokeLinecap="round">
    <circle cx="12" cy="12" r="9" />
    <path d="M8.5 14.5a4.5 4.5 0 0 0 7 0" />
    <circle cx="9" cy="10" r=".8" fill="currentColor" />
    <circle cx="15" cy="10" r=".8" fill="currentColor" />
  </svg>
);
export const StickerIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2} strokeLinejoin="round">
    <path d="M20.5 13.5V7a3.5 3.5 0 0 0-3.5-3.5H7A3.5 3.5 0 0 0 3.5 7v10A3.5 3.5 0 0 0 7 20.5h6.5l7-7Z" />
    <path d="M13.5 20.5v-4a3 3 0 0 1 3-3h4" />
    <circle cx="9" cy="10" r=".9" fill="currentColor" />
    <circle cx="15" cy="10" r=".9" fill="currentColor" />
  </svg>
);
export const SendIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M3.4 20.6a.9.9 0 0 1-1.2-1.1L4.4 13l9.1-1-9.1-1-2.2-6.5a.9.9 0 0 1 1.2-1.1l18 8.1a.9.9 0 0 1 0 1.6l-18 8.1Z" />
  </svg>
);
export const DocIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M6.5 2A2.5 2.5 0 0 0 4 4.5v15A2.5 2.5 0 0 0 6.5 22h11a2.5 2.5 0 0 0 2.5-2.5V9h-5.5A2.5 2.5 0 0 1 12 6.5V2H6.5ZM14 2.5v4c0 .3.2.5.5.5h4.9L14 2.5Z" />
  </svg>
);
export const PhotoIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M5 4a3 3 0 0 0-3 3v10a3 3 0 0 0 3 3h14a3 3 0 0 0 3-3V7a3 3 0 0 0-3-3H5Zm3.5 3.5a2 2 0 1 1 0 4 2 2 0 0 1 0-4ZM4 17l4.5-4.5 3 3 4-5L20 16v1a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1Z" />
  </svg>
);
export const PlayIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M8 5.1v13.8a1 1 0 0 0 1.5.8l11-6.9a1 1 0 0 0 0-1.6l-11-6.9a1 1 0 0 0-1.5.8Z" />
  </svg>
);
export const PhoneIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M6.6 2.6a1.5 1.5 0 0 1 1.9.5l2 3.2a1.5 1.5 0 0 1-.3 1.9l-1.4 1.2a12 12 0 0 0 5.8 5.8l1.2-1.4a1.5 1.5 0 0 1 1.9-.3l3.2 2a1.5 1.5 0 0 1 .5 1.9l-1 2.1A2.5 2.5 0 0 1 18 21C10.3 21 3 13.7 3 6a2.5 2.5 0 0 1 1.5-2.4l2.1-1Z" />
  </svg>
);
export const VideoIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M4.5 5A2.5 2.5 0 0 0 2 7.5v9A2.5 2.5 0 0 0 4.5 19h9a2.5 2.5 0 0 0 2.5-2.5v-1.8l4.4 3a1 1 0 0 0 1.6-.8V6.1a1 1 0 0 0-1.6-.8l-4.4 3V7.5A2.5 2.5 0 0 0 13.5 5h-9Z" />
  </svg>
);
export const GroupIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm7.5 0a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM9 13c-3.6 0-6.5 1.8-6.5 4v1.5c0 .3.2.5.5.5h12c.3 0 .5-.2.5-.5V17c0-2.2-2.9-4-6.5-4Zm7.5 0c-.6 0-1.2.1-1.7.2 1.3.9 2.2 2.2 2.2 3.8v2h4.5c.3 0 .5-.2.5-.5V17c0-2.2-2.5-4-5.5-4Z" />
  </svg>
);
export const MegaphoneIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2} strokeLinejoin="round">
    <path d="M3.5 10v4a1 1 0 0 0 1 1H7l9 4.5v-15L7 9H4.5a1 1 0 0 0-1 1Z" />
    <path d="M7 15l1.2 4.5a1 1 0 0 0 1 .7h1.3M19 9.5a3 3 0 0 1 0 5" />
  </svg>
);
export const PersonPlusIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M10 12a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9Zm0 2c-4.4 0-8 2.2-8 5v1.5c0 .3.2.5.5.5h12.2A6 6 0 0 1 14 18c0-1.5.6-2.9 1.5-4-1.6-.6-3.5-1-5.5-1Zm9 0a1 1 0 0 1 1 1v2h2a1 1 0 1 1 0 2h-2v2a1 1 0 1 1-2 0v-2h-2a1 1 0 1 1 0-2h2v-2a1 1 0 0 1 1-1Z" />
  </svg>
);
export const LockIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M7 10V7.5a5 5 0 0 1 10 0V10h.5A1.5 1.5 0 0 1 19 11.5v8a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 19.5v-8A1.5 1.5 0 0 1 6.5 10H7Zm2 0h6V7.5a3 3 0 0 0-6 0V10Z" />
  </svg>
);
export const NoteIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinejoin="round">
    <rect x="5" y="3.5" width="14" height="17" rx="2" />
    <path d="M8.5 8h7M8.5 12h7M8.5 16h4" strokeLinecap="round" />
  </svg>
);
export const SpeakerIcon = (p: P) => (
  <svg {...base(p)}>
    <path d="M4 9.5v5a1 1 0 0 0 1 1h3l4.4 3.7a1 1 0 0 0 1.6-.8V5.6a1 1 0 0 0-1.6-.8L8 8.5H5a1 1 0 0 0-1 1Zm12.5-1.8a1 1 0 0 1 1.4 0 6 6 0 0 1 0 8.6 1 1 0 1 1-1.4-1.4 4 4 0 0 0 0-5.8 1 1 0 0 1 0-1.4Z" />
  </svg>
);
export const BookmarkIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={p.strokeWidth ?? 1.9} strokeLinejoin="round">
    <path d="M6.5 3.5h11a1 1 0 0 1 1 1V20l-6.5-4.2L5.5 20V4.5a1 1 0 0 1 1-1Z" />
  </svg>
);
export const SlidersIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round">
    <path d="M4 8h9M17 8h3M4 16h3M11 16h9" />
    <circle cx="15" cy="8" r="2" />
    <circle cx="9" cy="16" r="2" />
  </svg>
);
export const PhoneOutlineIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={2} strokeLinejoin="round">
    <path d="M6.6 3.6a1.5 1.5 0 0 1 1.9.5l1.8 2.8a1.5 1.5 0 0 1-.3 1.9l-1.3 1.1a11 11 0 0 0 5.4 5.4l1.1-1.3a1.5 1.5 0 0 1 1.9-.3l2.8 1.8a1.5 1.5 0 0 1 .5 1.9l-.8 1.7A2.5 2.5 0 0 1 17.2 20C10 20 4 14 4 6.8a2.5 2.5 0 0 1 1.5-2.3l1.1-.9Z" />
  </svg>
);

// The photo viewer's toolbar, after LINE for Mac's.
const line = (p: P) => ({
  ...base(p),
  fill: "none",
  stroke: "currentColor",
  strokeWidth: p.strokeWidth ?? 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
});
export const GridIcon = (p: P) => (
  <svg {...line(p)}>
    <circle cx="7.5" cy="7.5" r="3.5" />
    <circle cx="16.5" cy="7.5" r="3.5" />
    <circle cx="7.5" cy="16.5" r="3.5" />
    <circle cx="16.5" cy="16.5" r="3.5" />
  </svg>
);
export const MinusIcon = (p: P) => (
  <svg {...line(p)}>
    <path d="M4 12h16" />
  </svg>
);
export const PlusIcon = (p: P) => (
  <svg {...line(p)}>
    <path d="M4 12h16M12 4v16" />
  </svg>
);
export const FitIcon = (p: P) => (
  <svg {...line(p)}>
    <path d="M4 4l16 16M4 4v6M4 4h6M20 20v-6M20 20h-6" />
  </svg>
);
export const RotateIcon = (p: P) => (
  <svg {...line(p)}>
    <rect x="9" y="11" width="11" height="9" rx="1.5" />
    <path d="M4 15V9a4 4 0 0 1 4-4h5M10.5 2.5 13 5l-2.5 2.5" />
  </svg>
);
export const DownloadIcon = (p: P) => (
  <svg {...line(p)}>
    <path d="M12 3.5v11M7.5 10l4.5 4.5 4.5-4.5M4 15.5V20h16v-4.5" />
  </svg>
);
export const PauseIcon = (p: P) => (
  <svg {...base(p)}>
    <rect x="6" y="4.5" width="4" height="15" rx="1" />
    <rect x="14" y="4.5" width="4" height="15" rx="1" />
  </svg>
);
export const VolumeIcon = ({ muted, ...p }: P & { muted?: boolean }) => (
  <svg {...line(p)}>
    <path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor" />
    {muted ? <path d="m16 9.5 5 5M21 9.5l-5 5" /> : <path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" />}
  </svg>
);
export const DoubleChevronIcon = ({ up, ...p }: P & { up?: boolean }) => (
  <svg {...line(p)} style={{ transform: up ? "rotate(180deg)" : undefined }}>
    <path d="m7 7 5 4 5-4M7 13l5 4 5-4" />
  </svg>
);
export const CommentIcon = (p: P) => (
  <svg {...line(p)}>
    <path d="M12 3.5c-4.7 0-8.5 3.3-8.5 7.4 0 2.3 1.2 4.3 3.1 5.7l-.6 3.9 4-2.2c.6.1 1.3.2 2 .2 4.7 0 8.5-3.3 8.5-7.5S16.7 3.5 12 3.5Z" />
    <circle cx="8.5" cy="11" r=".6" fill="currentColor" />
    <circle cx="12" cy="11" r=".6" fill="currentColor" />
    <circle cx="15.5" cy="11" r=".6" fill="currentColor" />
  </svg>
);
export const BlockIcon = (p: P) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth={p.strokeWidth ?? 2.2} strokeLinecap="round">
    <circle cx="12" cy="12" r="8.5" />
    <path d="m6 18 12-12" />
  </svg>
);
export const WarningIcon = (p: P) => (
  <svg {...base(p)} fillRule="evenodd">
    <path d="M12 2.5c.6 0 1.1.3 1.4.8l9.2 16a1.6 1.6 0 0 1-1.4 2.4H2.8a1.6 1.6 0 0 1-1.4-2.4l9.2-16c.3-.5.8-.8 1.4-.8ZM11 9h2v6h-2V9Zm1 7.5a1.25 1.25 0 1 0 0 2.5 1.25 1.25 0 0 0 0-2.5Z" />
  </svg>
);
