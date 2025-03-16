//
// Created by Erich Essmann on 12/03/2025.
//
#pragma once

#include <algorithm>
#include <functional>
#include <iostream>
#include <ranges>
#include <type_traits>
#include <vector>

#include "rust/cxx.h"

namespace detail {
// Type trait to check if T is rust::Slice (including const-qualified elements)
template <typename T>
struct is_rust_slice : std::false_type {};

template <typename T>
struct is_rust_slice<rust::Slice<T>> : std::true_type {};

template <typename T>
struct is_rust_slice<rust::Slice<const T>> : std::true_type {};

// Type trait to get the element type of rust::Slice, removing const
template <typename T>
struct element_type {
  using type = void;
};

template <typename T>
struct element_type<rust::Slice<T>> {
  using type = T;
};

template <typename T>
struct element_type<rust::Slice<const T>> {
  using type = T;  // Remove the const qualifier
};

template <typename T>
using element_type_t = typename element_type<T>::type;

// Helper to remove const qualifier from a type
template <typename T>
struct remove_const_wrapper {
  using type = std::remove_const_t<T>;
};

template <typename T>
using remove_const_t = typename remove_const_wrapper<T>::type;

// Trait to detect if a type is a specialization of std::vector
template <typename T>
struct is_vector : std::false_type {};

template <typename T, typename Alloc>
struct is_vector<std::vector<T, Alloc>> : std::true_type {};

}  // namespace detail

namespace quest_helper {
template <typename T>
T* slice_to_ptr(rust::Slice<const T> slice) {
  return const_cast<T*>(slice.data());
}

// Recursive function applying the operation to the innermost elements
template <typename T, typename Func>
auto apply_deep(T&& container, Func&& func) {
  if constexpr (detail::is_vector<std::remove_cvref_t<T>>::value) {
    // If it's a vector, recursively apply views::transform
    return std::forward<T>(container) |
           std::views::transform([f = std::forward<Func>(func)](auto&& elem) {
             return apply_deep(std::forward<decltype(elem)>(elem), f);
           });
  } else {
    // If not a container, directly apply the function
    return std::invoke(std::forward<Func>(func), std::forward<T>(container));
  }
}

// Function to apply transformation in-place for better efficiency when needed
template <typename T, typename Func>
void apply_deep_in_place(T& container, Func&& func) {
  if constexpr (detail::is_vector<std::remove_cvref_t<T>>::value) {
    // If it's a vector, recursively apply to each element
    for (auto& elem : container) {
      apply_deep_in_place(elem, func);
    }
  } else {
    // If not a container, directly apply the function
    container = std::invoke(func, std::move(container));
  }
}

// Enhanced helper to materialize the transformed view into a concrete vector
// again
template <typename T>
auto to_vector(T&& range) {
  using value_type = std::ranges::range_value_t<std::remove_cvref_t<T>>;

  if constexpr (detail::is_vector<value_type>::value) {
    // Nested vector, recurse again
    std::vector<decltype(to_vector(*std::begin(range)))> result;

    // Reserve space if possible to avoid reallocations
    if constexpr (std::ranges::sized_range<std::remove_cvref_t<T>>) {
      result.reserve(std::ranges::size(range));
    }

    for (auto&& subrange : range) {
      result.push_back(to_vector(subrange));
    }
    return result;
  } else {
    // Base case, direct element
    std::vector<value_type> result;

    // Reserve space if possible
    if constexpr (std::ranges::sized_range<std::remove_cvref_t<T>>) {
      result.reserve(std::ranges::size(range));
    }

    std::ranges::copy(range, std::back_inserter(result));
    return result;
  }
}

// Improved helper function that combines transformation and materialization
template <typename T, typename Func>
auto transform_deep(T&& container, Func&& func) {
  auto transformed_view =
      apply_deep(std::forward<T>(container), std::forward<Func>(func));
  return to_vector(transformed_view);
}

// Alternative: Eager approach that builds result directly
template <typename T, typename Func>
auto transform_deep_eager(T&& container, Func&& func) {
  if constexpr (detail::is_vector<std::remove_cvref_t<T>>::value) {
    using inner_type = typename std::remove_cvref_t<T>::value_type;

    std::vector<decltype(transform_deep_eager(std::declval<inner_type&>(),
                                              func))>
        result;
    result.reserve(container.size());

    for (auto&& elem : container) {
      result.push_back(
          transform_deep_eager(std::forward<decltype(elem)>(elem), func));
    }
    return result;
  } else {
    return std::invoke(std::forward<Func>(func), std::forward<T>(container));
  }
}

template <typename SliceType>
auto slice_to_vector(const SliceType& slice) {
  static_assert(detail::is_rust_slice<SliceType>::value,
                "Input must be a rust::Slice");

  using RawElementType = detail::element_type_t<SliceType>;
  using ElementType = detail::remove_const_t<RawElementType>;

  if constexpr (std::is_same_v<ElementType, Quest_Complex>) {
    // Base case: rust::Slice<const Quest_Complex> -> std::vector<qcomp>
    std::vector<qcomp> result;
    result.reserve(slice.size());
    for (const auto& item : slice) {
      result.push_back(static_cast<qcomp>(item));
    }
    return result;
  } else if constexpr (detail::is_rust_slice<ElementType>::value ||
                       detail::is_rust_slice<const ElementType>::value) {
    // Recursive case: rust::Slice<rust::Slice<...>> ->
    // std::vector<std::vector<...>>
    std::vector<decltype(slice_to_vector(std::declval<ElementType>()))> result;
    result.reserve(slice.size());
    for (const auto& item : slice) {
      result.push_back(slice_to_vector(item));
    }
    return result;
  } else {
    // Invalid input type - will generate a helpful compile error
    static_assert(
        std::is_same_v<ElementType, Quest_Complex> ||
            detail::is_rust_slice<ElementType>::value ||
            detail::is_rust_slice<const ElementType>::value,
        "Slice must contain either Quest_Complex or nested rust::Slice");
    // This return is never reached but needed for compilation
    return std::vector<int>();
  }
}

}  // namespace quest_helper
