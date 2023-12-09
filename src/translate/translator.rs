/**
 * ! For the creation of this file we have relied on the GearBot2 translator.
 * ! You can visit the project by clicking on the following URL
 * ! ( https://github.com/gearbot/GearBot-2 )
 */
use std::{
  borrow::Cow,
  collections::HashMap,
  error::Error,
  fmt,
  fs::{self, DirEntry},
  io::Error as IoError,
};

use fluent_bundle::{bundle::FluentBundle, FluentArgs, FluentMessage, FluentResource, FluentValue};
use intl_memoizer::concurrent::IntlLangMemoizer;
use tracing::{debug, error, info, trace, warn};
use unic_langid::LanguageIdentifier;

pub const TRANSLATION_FAILED: &str = "An error has ocurred while trying to translate the message"; // Default error message if translation fails

pub type Bundle = FluentBundle<FluentResource, IntlLangMemoizer>;

// Translator error type
#[derive(Debug)]
pub struct TranslatorError {
  pub description: String,
  pub name: &'static str,
}
impl Error for TranslatorError {}
impl fmt::Display for TranslatorError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let description = &self.description;
    let name = self.name;
    write!(f, "{name}: {description}")
  }
}

pub trait LanguageKey {
  fn as_str(&self) -> &'static str;
}

// Translator structure
pub struct Translator {
  translations: HashMap<String, Bundle>,
  default_language: String,
}

// Message translator structure
pub struct MessageTranslator<'lifetime, Key: LanguageKey> {
  key: Key,
  bundle: &'lifetime Bundle,
  message: Option<FluentMessage<'lifetime>>,
  args: Option<FluentArgs<'lifetime>>,
}

impl Translator {
  // Creates a new translator object using directory path and indicating the default language
  pub fn new(
    language_directory: &str,
    default_language: String,
  ) -> Result<Translator, TranslatorError> {
    info!("Loading translations...");

    // Reads directory files
    let translations_directory = fs::read_dir(language_directory).map_err(|_| TranslatorError {
      name: "READ_DIR_ERROR",
      description: "An error has ocurred while reading translations directory".to_string(),
    })?;

    // Creates translations hashmap
    let mut translations = HashMap::new();

    for result in translations_directory {
      let directory_data = Self::get_directory_data(result)?; // Skips non directory files and returns it data and name
      if directory_data.is_none() {
        continue;
      }

      let (directory, directory_name) = directory_data.unwrap();

      // Extracts language identifiers from directory's name
      if let Ok(language_identifier) = directory_name.parse::<LanguageIdentifier>() {
        debug!("Loading translations for {}", directory_name);
        let langs = vec![language_identifier];
        let mut bundle = Bundle::new_concurrent(langs); // Creates new bundle to use locales

        let language_directory = fs::read_dir(directory.path()).map_err(|_| TranslatorError {
          name: "READ_DIR_ERROR",
          description: format!("An error has ocured while trying to read {language_directory}"),
        })?;

        for file_result in language_directory {
          let file = Self::get_file_data(file_result);

          if file.is_none() {
            continue;
          }

          let (content, file_name) = file.unwrap();

          // Creates translation resources using file content
          let resource = FluentResource::try_new(content);

          // Checks if file content is not corrupted
          match resource {
            Ok(resource) => {
              bundle.add_resource(resource).map_err(|_| TranslatorError {
                name: "BUNDLE_ERROR",
                description: format!("Could not add data from file {file_name} to bundle"),
              })?;
            }
            Err(error) => {
              error!(
                "Corrupt entry encountered in file {} from language {}: {:?}",
                file_name, directory_name, error.1
              );
            }
          }
        }

        // Adds translations to the hashmap
        translations.insert(directory_name.to_string(), bundle);
      } else {
        warn!(
          "Ignoring {} as it is not a valid language identifier",
          directory_name
        );
      }
    }

    info!("Successfully loaded {} languages", translations.len());

    // Checks if translations contains the default message
    if !translations.contains_key(&default_language) {
      return Err(TranslatorError {
        name: "DEFAULT_LANGUAGE_ERROR",
        description: format!("{default_language} was designated as default language, but no translations where provided for this language")
      });
    }

    Ok(Translator {
      translations,
      default_language,
    })
  }

  pub fn get_directory_data(
    directory_result: Result<DirEntry, IoError>,
  ) -> Result<Option<(DirEntry, String)>, TranslatorError> {
    let directory = directory_result.expect("Could not get directory data");
    let directory_name = directory.file_name().to_string_lossy().to_string();
    if !directory // ignores file if it's not a directory
      .file_type()
      .map_err(|_| TranslatorError {
        name: "READ_FILE_ERROR",
        description: format!("Could not get file type from {directory_name}"),
      })?
      .is_dir()
    {
      warn!("Ignoring {} because it is not a directory", directory_name);
      Ok(None)
    } else {
      Ok(Some((directory, directory_name)))
    }
  }

  pub fn get_file_data(file_result: Result<DirEntry, IoError>) -> Option<(String, String)> {
    let file = file_result.expect("Failed to get file metadata");
    let file_name = file.file_name().to_string_lossy().to_string();
    trace!("Loading file {}", file_name);

    match fs::read_to_string(file.path()) {
      Ok(content) => Some((content, file_name)),
      Err(error) => {
        error!(
          "An error has ocurred while reading file {}: {}",
          file_name, error
        );
        None
      }
    }
  }

  // Returns the translated message
  pub fn get_message<'lifetime, Key: LanguageKey>(
    &'lifetime self,
    language: &str,
    key: &Key,
  ) -> (Option<FluentMessage>, &'lifetime Bundle) {
    let translation_key = key.as_str();
    // Checks if translation language exists
    let (translations, language) = if let Some(translations) = self.translations.get(language) {
      (translations, language)
    } else {
      debug!(
        "Attempted to translate to unknown language {}, falling back to {}",
        language, self.default_language
      );
      (
        self.translations.get(&self.default_language).unwrap(),
        self.default_language.as_str(),
      )
    };

    // Gets translation message
    let mut message = translations.get_message(translation_key);

    // Cheks if there's no message and if translation language is equal to the default language
    if message.is_none() && language != self.default_language {
      message = self
        .translations
        .get(&self.default_language)
        .unwrap()
        .get_message(translation_key);
    }

    (message, translations)
  }

  // Creates the message and returns the message translator structure
  pub fn translate<Key: LanguageKey>(&self, language: &str, key: Key) -> MessageTranslator<Key> {
    let (message, bundle) = self.get_message(language, &key);

    MessageTranslator {
      key,
      bundle,
      message,
      args: Default::default(),
    }
  }

  // Translate the message without arguments
  pub fn translate_without_args<Key: LanguageKey>(&self, language: &str, key: Key) -> Cow<str> {
    let (message, bundle) = self.get_message(language, &key);
    // Checks if it's posible to translate the message
    if let Some(message) = message {
      let mut errors = Vec::new();
      let translated = bundle.format_pattern(message.value().unwrap(), None, &mut errors);
      // If there's not errors and the language is correct, returns the message

      if errors.is_empty() {
        translated
      } else {
        error!(
          "Translation failure(s) when translating {} without arguments: {:?}",
          key.as_str(),
          errors
        );
        Cow::Borrowed(TRANSLATION_FAILED)
      }
    } else {
      error!(
        "Tried to translate non existing language key: {}",
        key.as_str()
      );
      Cow::Borrowed(TRANSLATION_FAILED)
    }
  }
}

// Message translator implementations
impl<'lifetime, Key> MessageTranslator<'lifetime, Key>
where
  Key: LanguageKey,
{
  // Adds arguments to message translator
  pub fn add_argument<P>(mut self, key: &'lifetime str, value: P) -> Self
  where
    P: Into<FluentValue<'lifetime>>,
  {
    let mut args = match self.args {
      None => FluentArgs::new(),
      Some(args) => args,
    };
    args.set(key, value.into());
    self.args = Some(args);
    self
  }

  // Builds the message
  pub fn build(&self) -> Cow<str> {
    let mut errors = Vec::new();

    match &self.message {
      None => {
        error!(
          "Tried to translate non existing language key: {}",
          self.key.as_str()
        );
        Cow::Borrowed(TRANSLATION_FAILED)
      }
      Some(message) => {
        let translated =
          self
            .bundle
            .format_pattern(message.value().unwrap(), self.args.as_ref(), &mut errors);

        if errors.is_empty() {
          translated
        } else {
          error!(
            "Translation failure(s) when traslating {} with args {:?}: {:?}",
            self.key.as_str(),
            self.args,
            errors
          );
          Cow::Borrowed(TRANSLATION_FAILED)
        }
      }
    }
  }
}
