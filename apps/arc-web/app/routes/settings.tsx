import { useState } from "react";
import {
  Cog6ToothIcon,
  CodeBracketIcon,
  CpuChipIcon,
  BellIcon,
  ShieldCheckIcon,
} from "@heroicons/react/24/outline";
import type { Route } from "./+types/settings";

export function meta({}: Route.MetaArgs) {
  return [{ title: "Settings — Arc" }];
}

export const handle = { hideHeader: true };

interface SettingField {
  key: string;
  label: string;
  value: string;
  type: "text" | "select" | "toggle";
  options?: string[];
  description?: string;
}

interface SettingGroup {
  id: string;
  name: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  accentColor: string;
  fields: SettingField[];
}

const settingGroups: SettingGroup[] = [
  {
    id: "general",
    name: "General",
    description: "Core platform settings and defaults.",
    icon: Cog6ToothIcon,
    accentColor: "teal",
    fields: [
      { key: "org_name", label: "Organization name", value: "Acme Corp", type: "text" },
      { key: "default_branch", label: "Default branch", value: "main", type: "text" },
      { key: "timezone", label: "Timezone", value: "America/New_York", type: "select", options: ["America/New_York", "America/Chicago", "America/Denver", "America/Los_Angeles", "UTC", "Europe/London", "Europe/Berlin", "Asia/Tokyo"] },
      { key: "auto_cancel", label: "Auto-cancel superseded runs", value: "true", type: "toggle" },
    ],
  },
  {
    id: "git",
    name: "Git & VCS",
    description: "Version control integration and repository settings.",
    icon: CodeBracketIcon,
    accentColor: "mint",
    fields: [
      { key: "github_org", label: "GitHub organization", value: "acme-corp", type: "text" },
      { key: "clone_protocol", label: "Clone protocol", value: "SSH", type: "select", options: ["SSH", "HTTPS"] },
      { key: "auto_merge", label: "Auto-merge when checks pass", value: "false", type: "toggle" },
      { key: "delete_branch", label: "Delete branch after merge", value: "true", type: "toggle" },
      { key: "commit_signing", label: "Require commit signing", value: "false", type: "toggle" },
    ],
  },
  {
    id: "compute",
    name: "Compute",
    description: "Resource allocation and execution environment.",
    icon: CpuChipIcon,
    accentColor: "amber",
    fields: [
      { key: "default_cpu", label: "Default CPU", value: "4", type: "select", options: ["2", "4", "8", "16"] },
      { key: "default_memory", label: "Default memory", value: "8 GB", type: "select", options: ["4 GB", "8 GB", "16 GB", "32 GB"] },
      { key: "max_parallel", label: "Max parallel runs", value: "10", type: "text" },
      { key: "timeout_minutes", label: "Run timeout (minutes)", value: "120", type: "text" },
      { key: "gpu_enabled", label: "GPU acceleration", value: "false", type: "toggle" },
    ],
  },
  {
    id: "notifications",
    name: "Notifications",
    description: "Alerts and notification delivery preferences.",
    icon: BellIcon,
    accentColor: "teal",
    fields: [
      { key: "slack_webhook", label: "Slack webhook URL", value: "https://hooks.slack.com/services/T00/B00/xxxx", type: "text" },
      { key: "notify_on_failure", label: "Notify on failure", value: "true", type: "toggle" },
      { key: "notify_on_success", label: "Notify on success", value: "false", type: "toggle" },
      { key: "notify_on_approval", label: "Notify on approval needed", value: "true", type: "toggle" },
      { key: "email_digest", label: "Daily email digest", value: "false", type: "toggle" },
    ],
  },
  {
    id: "security",
    name: "Security",
    description: "Access control and security policies.",
    icon: ShieldCheckIcon,
    accentColor: "coral",
    fields: [
      { key: "sso_provider", label: "SSO provider", value: "Okta", type: "select", options: ["None", "Okta", "Azure AD", "Google Workspace", "OneLogin"] },
      { key: "mfa_required", label: "Require MFA", value: "true", type: "toggle" },
      { key: "session_timeout", label: "Session timeout", value: "8 hours", type: "select", options: ["1 hour", "4 hours", "8 hours", "24 hours", "7 days"] },
      { key: "audit_log", label: "Audit logging", value: "true", type: "toggle" },
      { key: "ip_allowlist", label: "IP allowlist", value: "", type: "text", description: "Comma-separated CIDRs. Leave empty to allow all." },
    ],
  },
];

const accentMap: Record<string, { icon: string; glow: string; toggle: string; border: string }> = {
  teal: {
    icon: "text-teal-500",
    glow: "bg-teal-500/8",
    toggle: "bg-teal-500",
    border: "border-teal-500/20",
  },
  mint: {
    icon: "text-mint",
    glow: "bg-mint/8",
    toggle: "bg-mint",
    border: "border-mint/20",
  },
  amber: {
    icon: "text-amber",
    glow: "bg-amber/8",
    toggle: "bg-amber",
    border: "border-amber/20",
  },
  coral: {
    icon: "text-coral",
    glow: "bg-coral/8",
    toggle: "bg-coral",
    border: "border-coral/20",
  },
};

function ToggleSwitch({ enabled, accent }: { enabled: boolean; accent: string }) {
  const colors = accentMap[accent];
  return (
    <button
      type="button"
      className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ${enabled ? colors.toggle : "bg-overlay-strong"}`}
      role="switch"
      aria-checked={enabled}
    >
      <span
        className={`pointer-events-none inline-block size-4 rounded-full bg-white shadow-sm transition-transform duration-200 ${enabled ? "translate-x-4" : "translate-x-0"}`}
      />
    </button>
  );
}

function SettingRow({ field, accent, isLast }: { field: SettingField; accent: string; isLast: boolean }) {
  return (
    <div className={`flex items-center justify-between gap-8 px-5 py-3.5 ${isLast ? "" : "border-b border-line"}`}>
      <div className="min-w-0">
        <p className="text-sm text-fg-2">{field.label}</p>
        {field.description != null && (
          <p className="mt-0.5 text-xs text-fg-muted">{field.description}</p>
        )}
      </div>

      <div className="shrink-0">
        {field.type === "toggle" ? (
          <ToggleSwitch enabled={field.value === "true"} accent={accent} />
        ) : field.type === "select" ? (
          <select
            defaultValue={field.value}
            className="appearance-none rounded-md border border-line bg-page/60 py-1.5 pl-3 pr-8 text-sm text-fg-2 outline-none transition-colors focus:border-focus focus:ring-0"
          >
            {field.options?.map((opt) => (
              <option key={opt} value={opt}>{opt}</option>
            ))}
          </select>
        ) : (
          <input
            type="text"
            defaultValue={field.value}
            className="w-64 rounded-md border border-line bg-page/60 px-3 py-1.5 text-sm text-fg-2 placeholder-fg-muted outline-none transition-colors focus:border-focus focus:ring-0"
          />
        )}
      </div>
    </div>
  );
}

function SettingsSection({ group, isActive, onVisible }: { group: SettingGroup; isActive: boolean; onVisible: () => void }) {
  const colors = accentMap[group.accentColor];
  const Icon = group.icon;

  return (
    <section
      id={`section-${group.id}`}
      ref={(el) => {
        if (!el) return;
        const observer = new IntersectionObserver(
          ([entry]) => { if (entry.isIntersecting) onVisible(); },
          { rootMargin: "-40% 0px -50% 0px" }
        );
        observer.observe(el);
        return () => observer.disconnect();
      }}
      className={`rounded-xl border transition-colors duration-300 ${
        isActive
          ? `${colors.border} bg-panel/60`
          : "border-line bg-panel/40"
      }`}
    >
      <div className="flex items-center gap-3 px-5 py-4">
        <div className={`flex items-center justify-center size-8 rounded-lg ${colors.glow}`}>
          <Icon className={`size-4.5 ${colors.icon}`} />
        </div>
        <div>
          <h2 className="text-sm font-semibold text-fg-2">{group.name}</h2>
          <p className="text-xs text-fg-muted">{group.description}</p>
        </div>
      </div>
      <div className="border-t border-line">
        {group.fields.map((field, i) => (
          <SettingRow
            key={field.key}
            field={field}
            accent={group.accentColor}
            isLast={i === group.fields.length - 1}
          />
        ))}
      </div>
    </section>
  );
}

function SidebarNav({ activeId }: { activeId: string }) {
  return (
    <nav className="sticky top-8 w-44 shrink-0 hidden lg:block">
      <p className="px-3 mb-3 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
        Settings
      </p>
      <ul className="space-y-0.5">
        {settingGroups.map((group) => {
          const colors = accentMap[group.accentColor];
          const Icon = group.icon;
          const active = activeId === group.id;
          return (
            <li key={group.id}>
              <a
                href={`#section-${group.id}`}
                onClick={(e) => {
                  e.preventDefault();
                  document.getElementById(`section-${group.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
                }}
                className={`flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm transition-all duration-200 ${
                  active
                    ? `${colors.glow} ${colors.icon} font-medium`
                    : "text-fg-3 hover:text-fg-2 hover:bg-overlay"
                }`}
              >
                <Icon className={`size-4 ${active ? colors.icon : "text-fg-muted"}`} />
                {group.name}
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

export default function Settings() {
  const [activeSection, setActiveSection] = useState(settingGroups[0].id);

  return (
    <div className="flex gap-10 items-start">
      <SidebarNav activeId={activeSection} />
      <div className="flex-1 min-w-0 space-y-5">
        {settingGroups.map((group) => (
          <SettingsSection
            key={group.id}
            group={group}
            isActive={activeSection === group.id}
            onVisible={() => setActiveSection(group.id)}
          />
        ))}
      </div>
    </div>
  );
}
