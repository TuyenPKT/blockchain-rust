import { colors, fonts } from "../theme";

interface StatCardProps {
  icon:    string;
  label:   string;
  value:   string | number;
  color?:  string;
  sub?:    string;
}

export function StatCard({ icon, label, value, color, sub }: StatCardProps) {
  return (
    <div style={{
      background: colors.surface,
      border: `1px solid ${colors.border}`,
      borderRadius: 12, padding: "18px 20px",
      display: "flex", flexDirection: "column", gap: 8,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{
          width: 32, height: 32, borderRadius: 8, fontSize: 16,
          background: colors.surface2, display: "flex",
          alignItems: "center", justifyContent: "center",
        }}>{icon}</span>
        <span style={{ fontSize: 12, color: colors.muted, fontWeight: 600, textTransform: "uppercase", letterSpacing: ".06em" }}>
          {label}
        </span>
      </div>
      <div style={{
        fontSize: 26, fontWeight: 700,
        color: color ?? colors.text,
        fontFamily: fonts.mono,
        letterSpacing: "-.02em",
      }}>{value}</div>
      {sub && <div style={{ fontSize: 12, color: colors.muted }}>{sub}</div>}
    </div>
  );
}
