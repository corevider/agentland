/// Whether a name is safe to hand to a scaffolder as an argument.
///
/// The core decides this again before anything runs — this is here so a person
/// is told at the keystroke rather than after a round trip, and the two rules
/// are deliberately the same one.
export function name_trouble(name: string): string | null {
    if (name.length === 0) {
        return "a project needs a name";
    }

    if (name.length > 64) {
        return "a project name is at most 64 characters";
    }

    if (!/^[a-z0-9]/.test(name)) {
        return "it starts with a lowercase letter or a digit";
    }

    if (!/^[a-z0-9._-]+$/.test(name)) {
        return "lowercase letters, digits, dash, underscore and dot only";
    }

    if (name.includes("..")) {
        return 'no ".." in a project name';
    }

    return null;
}
