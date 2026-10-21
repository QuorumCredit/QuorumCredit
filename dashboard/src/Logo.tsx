import React from "react";

interface LogoProps {
  size?: number;
  className?: string;
}

export const Logo: React.FC<LogoProps> = ({ size = 32, className = "" }) => {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 128 128"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      role="img"
      aria-label="QuorumCredit logo"
    >
      {/* Outer circle - trust network */}
      <circle cx="64" cy="64" r="60" fill="none" stroke="#2563eb" strokeWidth="2" opacity="0.2" />

      {/* Inner circle - core */}
      <circle cx="64" cy="64" r="40" fill="none" stroke="#2563eb" strokeWidth="2.5" />

      {/* Center circle - quorum */}
      <circle cx="64" cy="64" r="20" fill="#2563eb" />

      {/* Network nodes */}
      <g fill="#2563eb">
        <circle cx="64" cy="30" r="5" /> {/* Top */}
        <circle cx="94" cy="50" r="5" /> {/* Top right */}
        <circle cx="94" cy="78" r="5" /> {/* Bottom right */}
        <circle cx="64" cy="98" r="5" /> {/* Bottom */}
        <circle cx="34" cy="78" r="5" /> {/* Bottom left */}
        <circle cx="34" cy="50" r="5" /> {/* Top left */}
      </g>

      {/* Connection lines */}
      <g stroke="#2563eb" strokeWidth="1.5" opacity="0.6">
        <line x1="64" y1="30" x2="64" y2="64" />
        <line x1="94" y1="50" x2="64" y2="64" />
        <line x1="94" y1="78" x2="64" y2="64" />
        <line x1="64" y1="98" x2="64" y2="64" />
        <line x1="34" y1="78" x2="64" y2="64" />
        <line x1="34" y1="50" x2="64" y2="64" />
      </g>

      {/* Trust indicator - checkmark in center */}
      <g stroke="white" strokeWidth="2.5" fill="none" strokeLinecap="round" strokeLinejoin="round">
        <path d="M 58 64 L 62 68 L 70 60" />
      </g>
    </svg>
  );
};

export default Logo;
