import { useEffect, useState, type KeyboardEvent } from "react";
import { Input } from "@/components/ui/input";

interface MemberNumberInputProps {
  value: number;
  label: string;
  disabled: boolean;
  onCommit: (value: number) => void;
}

/** Compact integer editor: commit on blur/Enter, reset invalid input or Escape. */
export function MemberNumberInput({ value, label, disabled, onCommit }: MemberNumberInputProps) {
  const [draft, setDraft] = useState(String(value));

  useEffect(() => setDraft(String(value)), [value]);

  const commit = () => {
    const parsed = Number(draft);
    if (draft.trim() === "" || !Number.isInteger(parsed)) {
      setDraft(String(value));
    } else if (parsed !== value) {
      onCommit(parsed);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      event.currentTarget.blur();
    } else if (event.key === "Escape") {
      event.preventDefault();
      setDraft(String(value));
    }
  };

  return (
    <Input
      type="number"
      step="1"
      value={draft}
      disabled={disabled}
      aria-label={label}
      className="h-7 w-20"
      onClick={(event) => event.stopPropagation()}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={onKeyDown}
    />
  );
}
