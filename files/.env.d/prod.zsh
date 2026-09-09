# prod env profile — sourced by `envp`. Prompt shows red for prod kube contexts.
export AWS_PROFILE="prod"
export AWS_REGION="eu-west-1"
export AWS_DEFAULT_REGION="$AWS_REGION"
# Own kubeconfig for this bundle, so `use-context` cannot leak to other shells.
export KUBECONFIG="$HOME/.kube/config.env-prod"
[ -f "$KUBECONFIG" ] || { mkdir -p "$HOME/.kube"; cp "$HOME/.kube/config" "$KUBECONFIG" 2>/dev/null || : ; chmod 600 "$KUBECONFIG" 2>/dev/null || : ; }
kubectl config use-context prod >/dev/null 2>&1 || true
print -P "%F{red}⚠️  PROD profile active — commands hit production.%f"
