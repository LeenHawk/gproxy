import React from "react";

type JsonBlockProps = {
  value: unknown;
};

export default function JsonBlock({ value }: JsonBlockProps) {
  return (
    <pre className="whitespace-pre-wrap rounded-xl border border-slate-200 bg-slate-50 p-3 text-xs text-slate-700">
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}
