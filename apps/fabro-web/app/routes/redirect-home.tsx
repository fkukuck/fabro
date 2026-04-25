import { useEffect } from "react";
import { useNavigate } from "react-router";
import { ApiError } from "../lib/api-client";
import { useAuthMe } from "../lib/queries";

export default function RedirectHome() {
  const navigate = useNavigate();
  const { data, error } = useAuthMe();

  useEffect(() => {
    if (data) {
      navigate("/runs", { replace: true });
      return;
    }

    if (error instanceof ApiError && error.status === 401) {
      navigate("/login", { replace: true });
    }
  }, [data, error, navigate]);

  return null;
}
