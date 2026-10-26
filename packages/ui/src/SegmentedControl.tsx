import { ToggleGroup } from "radix-ui";

interface SegmentedControlProps<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  "aria-label": string;
}

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  "aria-label": ariaLabel,
}: SegmentedControlProps<T>) {
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      // Radix emite "" al pulsar la opción activa; se ignora para que siempre haya una.
      onValueChange={(next) => next && onChange(next as T)}
      aria-label={ariaLabel}
      className="inline-flex rounded-md border border-border bg-surface-2 p-0.5"
    >
      {options.map((opt) => (
        <ToggleGroup.Item
          key={opt.value}
          value={opt.value}
          className="h-7 rounded px-3 text-sm text-muted transition-colors hover:text-text focus-visible:outline-2 focus-visible:outline-focus data-[state=on]:bg-surface data-[state=on]:text-text data-[state=on]:shadow-sm"
        >
          {opt.label}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}
