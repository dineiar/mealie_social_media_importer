import type { ReactNode } from "react";
import { Brand } from "./Brand";

export function Header({ children }: { children?: ReactNode }) {
  return (
    <header className="site-header">
      <div className="site-header__inner">
        <Brand />
        {children}
      </div>
    </header>
  );
}
