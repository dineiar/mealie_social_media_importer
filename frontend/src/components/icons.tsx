import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement>;

function StrokeIcon({ strokeWidth = 1.8, children, ...props }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

export function KeyIcon(props: IconProps) {
  return (
    <StrokeIcon {...props}>
      <circle cx="8" cy="15" r="4" />
      <path d="m10.85 12.15 9.15-9.15M17 6l3 3M14.5 8.5l2.5 2.5" />
    </StrokeIcon>
  );
}

export function EyeIcon(props: IconProps) {
  return (
    <StrokeIcon {...props}>
      <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z" />
      <circle cx="12" cy="12" r="3" />
    </StrokeIcon>
  );
}

export function EyeOffIcon(props: IconProps) {
  return (
    <StrokeIcon {...props}>
      <path d="M9.9 5.2A10 10 0 0 1 12 5c6.4 0 10 7 10 7a17 17 0 0 1-2.6 3.4M6.5 6.6C3.7 8.4 2 12 2 12s3.6 7 10 7a9.6 9.6 0 0 0 4.4-1.1" />
      <path d="M9.9 9.9a3 3 0 0 0 4.2 4.2" />
      <path d="m3 3 18 18" />
    </StrokeIcon>
  );
}

export function AlertCircleIcon(props: IconProps) {
  return (
    <StrokeIcon strokeWidth={2} {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7.5v5.5M12 16.5h.01" />
    </StrokeIcon>
  );
}

export function ArrowRightIcon(props: IconProps) {
  return (
    <StrokeIcon strokeWidth={2} {...props}>
      <path d="M5 12h14M13 6l6 6-6 6" />
    </StrokeIcon>
  );
}

export function LogOffIcon(props: IconProps) {
  return (
    <StrokeIcon {...props}>
      <path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3" />
      <path d="M10 17l-5-5 5-5M5 12h11" />
    </StrokeIcon>
  );
}

export function RefreshIcon(props: IconProps) {
  return (
    <StrokeIcon strokeWidth={2} {...props}>
      <path d="M20 11a8 8 0 0 0-14.3-4.9L4 8" />
      <path d="M4 4v4h4" />
      <path d="M4 13a8 8 0 0 0 14.3 4.9L20 16" />
      <path d="M20 20v-4h-4" />
    </StrokeIcon>
  );
}

export function SpinnerIcon(props: IconProps) {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true" className="spin" {...props}>
      <circle
        cx="10"
        cy="10"
        r="7.5"
        fill="none"
        stroke="currentColor"
        strokeOpacity=".35"
        strokeWidth="2.5"
      />
      <path
        d="M10 2.5a7.5 7.5 0 0 1 7.5 7.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
      />
    </svg>
  );
}
