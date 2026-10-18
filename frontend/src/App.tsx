import { AppProviders } from "@/app/providers";
import { ThemeToggle } from "@/components/theme/theme-toggle";

export function App() {
  return (
    <AppProviders>
      <main className="mx-auto max-w-3xl px-4 py-10">
        <div className="flex items-center justify-between gap-4">
          <h1 className="text-3xl font-semibold">iapolitique</h1>
          <ThemeToggle />
        </div>
      </main>
    </AppProviders>
  );
}
