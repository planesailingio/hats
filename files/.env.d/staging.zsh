# staging env profile — sourced by `envp`.
export AWS_PROFILE="staging"
export AWS_REGION="eu-west-2"
export AWS_DEFAULT_REGION="$AWS_REGION"
# Own kubeconfig for this bundle, so `use-context` cannot leak to other shells.
export KUBECONFIG="$HOME/.kube/config.env-staging"
[ -f "$KUBECONFIG" ] || { mkdir -p "$HOME/.kube"; cp "$HOME/.kube/config" "$KUBECONFIG" 2>/dev/null || : ; chmod 600 "$KUBECONFIG" 2>/dev/null || : ; }
kubectl config use-context staging >/dev/null 2>&1 || true
