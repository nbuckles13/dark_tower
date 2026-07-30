// Mutually recursive types are legal and idiomatic TS. Without a visited set the
// fixpoint does not terminate — and a hang emits no VIOLATION line, just
// STATUS=FAIL REASON=guard-timeout after 30s on every devloop repo-wide, presenting as
// a PERFORMANCE problem so triage goes to file counts rather than to a recursive type.
// Credential-free, so the correct result is silence AND termination.
export interface TreeNode {
  readonly child: TreeLeaf;
}
export interface TreeLeaf {
  readonly parent: TreeNode;
  readonly label: string;
}
