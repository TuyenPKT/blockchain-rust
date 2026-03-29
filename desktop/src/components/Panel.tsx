import { ReactNode } from "react";
import { colors, fonts } from "../theme";

interface PanelProps {
  icon?:     string;
  title:     string;
  right?:    ReactNode;
  children:  ReactNode;
  style?:    React.CSSProperties;
}

export function Panel({ icon, title, right, children, style }: PanelProps) {
  return (
    <div style={{
      background: colors.surface,
      border: `1px solid ${colors.border}`,
      borderRadius: 14, overflow: "hidden", ...style,
    }}>
      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 10,
        padding: "14px 18px",
        borderBottom: `1px solid ${colors.border}`,
      }}>
        {icon && <span style={{ fontSize: 16 }}>{icon}</span>}
        <span style={{
          fontWeight: 700, fontSize: 14, color: colors.text,
          flex: 1, fontFamily: fonts.sans,
        }}>{title}</span>
        {right}
      </div>
      {children}
    </div>
  );
}
