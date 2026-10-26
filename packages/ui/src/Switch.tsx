import { Switch as RadixSwitch } from "radix-ui";

interface SwitchProps {
  id?: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

export function Switch({ id, checked, onCheckedChange }: SwitchProps) {
  return (
    <RadixSwitch.Root
      id={id}
      checked={checked}
      onCheckedChange={onCheckedChange}
      className="relative h-5 w-9 rounded-full bg-border transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[state=checked]:bg-accent"
    >
      <RadixSwitch.Thumb className="block size-4 translate-x-0.5 rounded-full bg-surface shadow transition-transform data-[state=checked]:translate-x-[18px]" />
    </RadixSwitch.Root>
  );
}
