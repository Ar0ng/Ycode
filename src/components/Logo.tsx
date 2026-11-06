import logo from "../assets/logo.png";
import previewLogo from "../../src-tauri/icons-preview/128x128@2x.png";

export function Logo({ size = 28, className = "" }: { size?: number; className?: string }) {
  return <img src={import.meta.env.VITE_INTERNAL_PREVIEW === "true" ? previewLogo : logo} width={size} height={size} alt="Ycode" draggable={false} className={`shrink-0 select-none ${className}`} />;
}
