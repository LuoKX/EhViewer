package com.hippo.ehviewer.bridge.updater

import android.os.Build
import com.hippo.ehviewer.BuildConfig

internal data class UpdateVariant(
  val flavor: String,
  val abis: List<String>,
)

internal fun installedUpdateVariant() =
  UpdateVariant(BuildConfig.FLAVOR, listOf(Build.SUPPORTED_ABIS[0]))

internal fun UpdateVariant.facts(operation: UpdateOperation = UpdateOperation.CHECK) =
  UpdatePolicyRequest(operation = operation, flavor = flavor, abis = abis)

/** Native owns ordering/selection; Android executes only caller-supplied predicate effects. */
internal fun selectUpdateBinary(
  names: List<String>,
  artifacts: Boolean,
  matchesVariant: ((String) -> Boolean)?,
  variant: () -> UpdateVariant = ::installedUpdateVariant,
  planner: (UpdatePolicyRequest) -> UpdatePolicyPlan = ::planUpdatePolicy,
): Int {
  val snapshot = names.toList()
  val operation =
    if (artifacts) UpdateOperation.SELECT_ARTIFACTS else UpdateOperation.SELECT_RELEASE
  val request =
    if (matchesVariant == null && snapshot.isNotEmpty()) {
      variant().facts(operation).copy(names = snapshot)
    } else {
      UpdatePolicyRequest(operation = operation, names = snapshot)
    }
  if (matchesVariant == null || snapshot.isEmpty()) return checkNotNull(planner(request).index)
  val prepared =
    request.copy(
      operation =
        if (artifacts) UpdateOperation.PREPARE_ARTIFACTS else UpdateOperation.PREPARE_RELEASE
    )
  val order = checkNotNull(planner(prepared).order)
  // Validate complete permutation before invoking any callback, even with an injected planner.
  require(
    order.size == snapshot.size &&
      order.all { it in snapshot.indices } &&
      order.toSet().size == snapshot.size
  ) {
    "Invalid update predicate order"
  }
  require(
    if (artifacts)
      order.zipWithNext().all { (left, right) ->
        val comparison = snapshot[left].compareTo(snapshot[right])
        comparison < 0 || comparison == 0 && left < right
      }
    else order.withIndex().all { (index, value) -> index == value }
  ) {
    "Invalid update predicate ordering"
  }
  val observations = ArrayList<Boolean>(order.size)
  for (index in order) {
    val matched = matchesVariant(snapshot[index])
    observations.add(matched)
    if (matched) break
  }
  val result = planner(request.copy(matches = observations))
  val expected = if (observations.last()) order[observations.lastIndex] else order[0]
  require(result.decision == UpdateDecision.SELECTED && result.index == expected) {
    "Invalid update predicate selection"
  }
  return checkNotNull(result.index)
}
