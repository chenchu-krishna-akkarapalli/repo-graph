---
name: codebase-design
description: Architecture design vocabulary and principles for deepening modules, designing seams, and cutting complexity.
---

# Codebase Design Vocabulary & Principles

This reference defines the shared design vocabulary for architectural reviews and refactoring workflows. Use these terms exactly in discussions, reports, and code reviews without substituting imprecise synonyms.

---

## 1. Core Architecture Vocabulary

| Term | Definition | What to Avoid |
| :--- | :--- | :--- |
| **Module** | A cohesive unit of code hiding an implementation behind a well-defined interface. | Do *not* call it a "component", "service", "unit", or "layer". |
| **Interface** | The public surface area (functions, methods, types) exposed by a module to its callers. | Do *not* call it an "API", "signature", or "wrapper". |
| **Depth (Deep Module)** | An interface that is simple and narrow while the implementation underneath handles significant complexity. Maximum leverage. | Avoid shallow modules where the interface is as complex as the implementation. |
| **Shallow Module** | An interface that does little more than pass arguments through to underlying calls. Adds cognitive overhead without reducing complexity. | Delete or collapse shallow modules into deeper ones. |
| **Seam** | A place where you can alter program behavior without changing the calling code. | Do *not* call it a "boundary" or "hook". |
| **Adapter** | A concrete implementation of an interface connecting a module to a specific runtime environment or dependency (e.g. SQLite adapter, HTTP client, In-memory mock). | Do *not* build hypothetical adapters without concrete need. |
| **Leverage** | The ratio of value/behavior provided by a module relative to the cognitive cost of learning its interface. | Deep modules maximize leverage; shallow ones waste budget. |
| **Locality** | Keeping logically related behavior physically close so understanding and modifying a feature does not require bouncing across dozens of files. | Avoid fragmenting simple logic across multiple single-method wrappers. |

---

## 2. Guiding Principles

### The Deletion Test
To evaluate whether a module is shallow or deep, ask:
> *If we deleted this module, would complexity concentrate, or just move?*
- **Concentrates:** The module was deep and valuable.
- **Just Moves / Disappears:** The module was a pass-through wrapper; delete or collapse it.

### Seams & Adapters Law
> *"One adapter = hypothetical seam. Two adapters = real seam."*
- Never create complex abstractions or interfaces for a single implementation unless a second adapter (e.g., a fast in-memory test adapter or alternative driver) actually exists.

### The Interface is the Test Surface
- Tests should target the module's public interface, not its private internal helpers. Deep modules provide stable, high-value test surfaces that survive internal refactoring.
