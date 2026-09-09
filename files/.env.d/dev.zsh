# dev env profile — sourced by `envp`. Edit AWS profile / kube context / region to match your setup.
export AWS_PROFILE="dev"
export AWS_REGION="eu-west-2"
export AWS_DEFAULT_REGION="$AWS_REGION"
# Per-shell kubeconfig so switching env in one terminal never leaks to another.
# Own kubeconfig for this bundle, so `use-context` cannot leak to other shells.
export KUBECONFIG="$HOME/.kube/config.env-dev"
[ -f "$KUBECONFIG" ] || { mkdir -p "$HOME/.kube"; cp "$HOME/.kube/config" "$KUBECONFIG" 2>/dev/null || : ; chmod 600 "$KUBECONFIG" 2>/dev/null || : ; }
# Switch kube context if it exists locally (ignore failure on machines without it).
kubectl config use-context dev >/dev/null 2>&1 || true
# Add any TF_VAR_* / tool env specific to dev below.
