import { KeyRoundIcon, LogInIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useDocumentTitle } from "@/hooks/use-document-title";

interface AdminSignInProps {
  onSubmit: (token: string) => void;
  /** The previous token was refused by the API. */
  rejected: boolean;
}

export function AdminSignIn({ onSubmit, rejected }: AdminSignInProps) {
  useDocumentTitle("Connexion · Administration");
  const id = useId();
  const [token, setToken] = useState("");
  const [empty, setEmpty] = useState(false);

  const handleSubmit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = token.trim();
    setEmpty(value === "");
    if (value !== "") onSubmit(value);
  };

  return (
    <Card className="mx-auto w-full max-w-md">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <KeyRoundIcon className="size-5" aria-hidden="true" />
          <h1 className="text-xl">Accès à l’administration</h1>
        </CardTitle>
        <CardDescription>
          Saisissez le jeton d’administration. Il reste dans cet onglet uniquement et n’est envoyé
          qu’à l’API d’administration.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} noValidate className="space-y-4">
          {rejected ? (
            <Alert variant="destructive" role="alert">
              <AlertTitle>Jeton refusé</AlertTitle>
              <AlertDescription>
                <p>Ce jeton n’est pas valide. Vérifiez-le puis réessayez.</p>
              </AlertDescription>
            </Alert>
          ) : null}
          <div className="space-y-2">
            <Label htmlFor={`${id}-token`}>Jeton d’administration</Label>
            <Input
              id={`${id}-token`}
              type="password"
              autoComplete="current-password"
              value={token}
              onChange={(event) => {
                setToken(event.target.value);
              }}
              aria-invalid={empty ? true : undefined}
              aria-describedby={empty ? `${id}-error` : undefined}
            />
            {empty ? (
              <p id={`${id}-error`} role="alert" className="text-sm font-medium text-destructive">
                Saisissez le jeton.
              </p>
            ) : null}
          </div>
          <Button type="submit" className="w-full">
            <LogInIcon aria-hidden="true" />
            Se connecter
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
