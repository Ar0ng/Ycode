import logo from "../assets/logo.png";

export function Logo({ size = 28, className = "" }: { size?: number; className?: string }) {
  return <img src={logo} width={size} height={size} alt="Ycode" draggable={false} className={`shrink-0 select-none ${className}`} />;
}
