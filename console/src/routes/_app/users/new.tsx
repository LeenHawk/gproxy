import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { UserForm } from "@/components/identity/user-form";

export const Route = createFileRoute("/_app/users/new")({
  component: NewUserPage,
});

function NewUserPage() {
  const { t } = useTranslation("identity");
  const navigate = useNavigate();

  return (
    <div className="grid gap-5 p-4 md:p-6">
      <h2 className="text-xl font-semibold">{t("users.new")}</h2>
      <div className="max-w-xl rounded-lg border p-4">
        <UserForm
          onSaved={(user) => {
            void navigate({
              to: "/users/$userId",
              params: { userId: String(user.id) },
              search: { tab: "profile" },
            });
          }}
        />
      </div>
    </div>
  );
}
