/** Portable client implementation of the configured receipt capability. */
export default function receipt(args) {
  return { receipt: args.value, execution: "browser" };
}
