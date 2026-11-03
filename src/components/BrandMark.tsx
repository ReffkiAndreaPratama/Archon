import { cx } from "../lib/format";

interface BrandMarkProps {
  size?: number;
  className?: string;
}

export function BrandMark({ size = 36, className }: BrandMarkProps) {
  return (
    <svg
      className={cx("brand-mark", className)}
      width={size}
      height={size}
      viewBox="0 0 48 48"
      role="img"
      aria-label="Archon"
    >
      <defs>
        <linearGradient id="archon-mark-fill" x1="8" y1="6" x2="40" y2="43">
          <stop offset="0" stopColor="#ffd39a" />
          <stop offset="0.52" stopColor="#f2a65a" />
          <stop offset="1" stopColor="#d46b35" />
        </linearGradient>
        <linearGradient id="archon-mark-bg" x1="4" y1="2" x2="44" y2="46">
          <stop offset="0" stopColor="#1c232c" />
          <stop offset="1" stopColor="#0d1116" />
        </linearGradient>
      </defs>
      <rect x="1.5" y="1.5" width="45" height="45" rx="12" fill="url(#archon-mark-bg)" />
      <rect
        x="1.5"
        y="1.5"
        width="45"
        height="45"
        rx="12"
        fill="none"
        stroke="rgba(255,255,255,0.10)"
      />
      <path
        d="M12.5 36.5V23.8c0-7.1 5.1-12.3 11.5-12.3s11.5 5.2 11.5 12.3v12.7"
        fill="none"
        stroke="url(#archon-mark-fill)"
        strokeWidth="3.6"
        strokeLinecap="round"
      />
      <path
        d="M19.3 36.5V24.9c0-3.2 2-5.6 4.7-5.6s4.7 2.4 4.7 5.6v11.6"
        fill="none"
        stroke="url(#archon-mark-fill)"
        strokeWidth="2"
        strokeLinecap="round"
        opacity="0.48"
      />
      <path d="M24 13.7v5.6" stroke="#ffe3be" strokeWidth="2" strokeLinecap="round" />
      <path d="M12.5 36.5h23" stroke="#ffe3be" strokeWidth="2.4" strokeLinecap="round" opacity="0.9" />
      <circle cx="24" cy="23.5" r="1.9" fill="#ffe3be" />
    </svg>
  );
}
