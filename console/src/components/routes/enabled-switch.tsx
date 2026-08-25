import { useMutation } from "@tanstack/react-query"
import { toast } from "sonner"
import { Switch } from "@/components/ui/switch"

export function EnabledSwitch({
  checked,
  label,
  errorMessage,
  onChange,
  onChanged,
}: {
  checked: boolean
  label: string
  errorMessage: string
  onChange: (enabled: boolean) => Promise<unknown>
  onChanged: () => void
}) {
  const mutation = useMutation({
    mutationFn: (enabled: boolean) => onChange(enabled),
    onSuccess: onChanged,
    onError: () => toast.error(errorMessage),
  })
  const displayed = mutation.isPending ? mutation.variables : checked

  return (
    <Switch
      checked={displayed}
      disabled={mutation.isPending}
      aria-label={label}
      onCheckedChange={(enabled) => mutation.mutate(enabled)}
    />
  )
}
