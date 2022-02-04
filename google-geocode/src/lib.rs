/*!
 * A rust library for interacting with the Geocode API.
 *
 * For more information, the Geocode API documentation is available at:
 * https://developers.google.com/maps/documentation/geocoding/overview
 *
 * Example:
 *
 * ```ignore
 * use google_geocode::Geocode;
 * use serde::{Deserialize, Serialize};
 *
 * async fn geocode() {
 *     // Initialize the Geocode client.
 *     let geocode = Geocode::new_from_env();
 *
 *     // Get geolocation data.
 *     let g = geocode.get("some address").await.unwrap();
 *
 *     println!("{:?}", g);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::{
    env, error,
    fmt::{self, Debug, Display, Formatter},
    hash::Hash,
    sync::Arc,
};

use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Geocode API.
const ENDPOINT: &str = "https://maps.google.com/maps/api/geocode/json";

/// Entrypoint for interacting with the Geocode API.
pub struct Geocode {
    key: String,

    client: Arc<Client>,
}

impl Geocode {
    /// Create a new Geocode client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<K>(key: K) -> Self
    where
        K: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Geocode client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("GOOGLE_GEOCODE_API_KEY").unwrap();

        Geocode::new(key)
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self
            .client
            .request(method.clone(), url)
            .headers(headers)
            .basic_auth(&self.key, Some(""));

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// Get information for an address.
    pub async fn get(&self, address: &str) -> Result<Reply, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "",
            (),
            Some(vec![("address", address.to_string()), ("key", self.key.to_string())]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let r: ReplyResult = resp.json().await.unwrap();
        if r.results.is_empty() {
            return Err(APIError {
                status_code: StatusCode::NOT_FOUND,
                body: "".to_string(),
            });
        }
        Ok(r.results.get(0).unwrap().clone())
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code, self.body
        )
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code, self.body
        )
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

/// One component of a separated address
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddressComponent {
    /// The full text description or name of the address component as returned by the Geocoder.
    #[serde(default)]
    long_name: String,
    /// An abbreviated textual name for the address component, if available.
    /// For example, an address component for the state of Alaska may have a long_name of "Alaska" and a short_name of "AK" using the 2-letter postal abbreviation.
    #[serde(default)]
    short_name: String,
    /// The type of the address component.
    #[serde(default)]
    types: Vec<String>,
}

/// Position information
#[derive(Debug, Clone, Deserialize)]
pub struct Geometry {
    /// The geocoded latitude, longitude value.
    /// For normal address lookups, this field is typically the most important.
    pub location: Coordinates,
    /// Stores additional data about the specified location
    pub location_type: LocationType,
    /// the recommended viewport for displaying the returned result, specified as two latitude,longitude values defining the southwest and northeast corner of the viewport bounding box. Generally the viewport is used to frame a result when displaying it to a user.
    pub viewport: Viewport,
    /// The bounding box which can fully contain the returned result.
    /// Note that these bounds may not match the recommended viewport. (For example, San Francisco includes the Farallon islands, which are technically part of the city, but probably should not be returned in the viewport.)
    pub bounds: Option<Viewport>,
}

/// What location Geometry refers to
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LocationType {
    /// Indicates that the returned result is a precise geocode
    /// for which we have location information accurate down to street address precision.
    Rooftop,

    /// Indicates that the returned result reflects an approximation (usually on a road)
    /// interpolated between two precise points (such as intersections).
    /// Interpolated results are generally returned when rooftop geocodes
    /// are unavailable for a street address.
    RangeInterpolated,

    /// Indicates that the returned result is the geometric center of a result
    /// such as a polyline (for example, a street) or polygon (region).
    GeometricCenter,

    /// Indicates that the returned result is approximate.
    Approximate,
}

/// A human-readable address of this location.
#[derive(Debug, Clone, Deserialize)]
pub struct FormattedAddress(String);

impl Display for FormattedAddress {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ReplyResult {
    #[serde(default)]
    pub error_message: String,
    #[serde(default)]
    pub results: Vec<Reply>,
    #[serde(default)]
    pub status: String,
}

/// A reply from the Google geocoding API
#[derive(Debug, Clone, Deserialize)]
pub struct Reply {
    /// The separate components applicable to this address.
    #[serde(default)]
    pub address_components: Vec<AddressComponent>,
    /// The human-readable address of this location.
    ///
    /// Often this address is equivalent to the postal address. Note that some countries, such as the United Kingdom, do not allow distribution of true postal addresses due to licensing restrictions.
    ///
    /// The formatted address is logically composed of one or more address components. For example, the address "111 8th Avenue, New York, NY" consists of the following components: "111" (the street number), "8th Avenue" (the route), "New York" (the city) and "NY" (the US state).
    ///
    /// Do not parse the formatted address programmatically. Instead you should use the individual address components, which the API response includes in addition to the formatted address field.
    pub formatted_address: FormattedAddress,
    /// Position information
    pub geometry: Geometry,
    /// A unique identifier that can be used with other Google APIs.
    pub place_id: PlaceId,
    /// All the localities contained in a postal code.
    /// This is only present when the result is a postal code that contains multiple localities.
    #[serde(default)]
    pub postcode_localities: Vec<String>,

    /// The type of the returned result. This array contains a set of zero or more tags identifying the type of feature returned in the result. For example, a geocode of "Chicago" returns "locality" which indicates that "Chicago" is a city, and also returns "political" which indicates it is a political entity.
    #[serde(default)]
    pub types: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Viewport {
    /// Northeast corner of the bounding box
    pub northeast: Coordinates,
    /// Southwest corner of the bounding box
    pub southwest: Coordinates,
}

/// Language that gets serialized as a language code
///
/// From https://developers.google.com/maps/faq#languagesupport
#[derive(Clone, Copy, Debug, Serialize)]
#[allow(dead_code)]
pub enum Language {
    /// Arabic (ar)
    #[serde(rename = "ar")]
    Arabic,
    /// Bulgarian (bg)
    #[serde(rename = "bg")]
    Bulgarian,
    /// Bengali (bn)
    #[serde(rename = "bn")]
    Bengali,
    /// Catalan (ca)
    #[serde(rename = "ca")]
    Catalan,
    /// Czech (cs)
    #[serde(rename = "cs")]
    Czech,
    /// Danish (da)
    #[serde(rename = "da")]
    Danish,
    /// German (de)
    #[serde(rename = "de")]
    German,
    /// Greek (el)
    #[serde(rename = "el")]
    Greek,
    /// English (en)
    #[serde(rename = "en")]
    English,
    /// EnglishAustralian (en-AU)
    #[serde(rename = "en-AU")]
    EnglishAustralian,
    /// EnglishGreatBritain (en-GB)
    #[serde(rename = "en-GB")]
    EnglishGreatBritain,
    /// Spanish (es)
    #[serde(rename = "es")]
    Spanish,
    /// Basque (eu)
    #[serde(rename = "eu")]
    Basque,
    /// Farsi (fa)
    #[serde(rename = "fa")]
    Farsi,
    /// Finnish (fi)
    #[serde(rename = "fi")]
    Finnish,
    /// Filipino (fil)
    #[serde(rename = "fil")]
    Filipino,
    /// French (fr)
    #[serde(rename = "fr")]
    French,
    /// Galician (gl)
    #[serde(rename = "gl")]
    Galician,
    /// Gujarati (gu)
    #[serde(rename = "gu")]
    Gujarati,
    /// Hindi (hi)
    #[serde(rename = "hi")]
    Hindi,
    /// Croatian (hr)
    #[serde(rename = "hr")]
    Croatian,
    /// Hungarian (hu)
    #[serde(rename = "hu")]
    Hungarian,
    /// Indonesian (id)
    #[serde(rename = "id")]
    Indonesian,
    /// Italian (it)
    #[serde(rename = "it")]
    Italian,
    /// Hebrew (iw)
    #[serde(rename = "iw")]
    Hebrew,
    /// Japanese (ja)
    #[serde(rename = "ja")]
    Japanese,
    /// Kannada (kn)
    #[serde(rename = "kn")]
    Kannada,
    /// Korean (ko)
    #[serde(rename = "ko")]
    Korean,
    /// Lithuanian (lt)
    #[serde(rename = "lt")]
    Lithuanian,
    /// Latvian (lv)
    #[serde(rename = "lv")]
    Latvian,
    /// Malayalam (ml)
    #[serde(rename = "ml")]
    Malayalam,
    /// Marathi (mr)
    #[serde(rename = "mr")]
    Marathi,
    /// Dutch (nl)
    #[serde(rename = "nl")]
    Dutch,
    /// Norwegian (no)
    #[serde(rename = "no")]
    Norwegian,
    /// Polish (pl)
    #[serde(rename = "pl")]
    Polish,
    /// Portuguese (pt)
    #[serde(rename = "pt")]
    Portuguese,
    /// PortugueseBrazil (pt-BR)
    #[serde(rename = "pt-BR")]
    PortugueseBrazil,
    /// PortuguesePortugal (pt-PT)
    #[serde(rename = "pt-PT")]
    PortuguesePortugal,
    /// Romanian (ro)
    #[serde(rename = "ro")]
    Romanian,
    /// Russian (ru)
    #[serde(rename = "ru")]
    Russian,
    /// Slovak (sk)
    #[serde(rename = "sk")]
    Slovak,
    /// Slovenian (sl)
    #[serde(rename = "sl")]
    Slovenian,
    /// Serbian (sr)
    #[serde(rename = "sr")]
    Serbian,
    /// Swedish (sv)
    #[serde(rename = "sv")]
    Swedish,
    /// Tamil (ta)
    #[serde(rename = "ta")]
    Tamil,
    /// Telugu (te)
    #[serde(rename = "te")]
    Telugu,
    /// Thai (th)
    #[serde(rename = "th")]
    Thai,
    /// Tagalog (tl)
    #[serde(rename = "tl")]
    Tagalog,
    /// Turkish (tr)
    #[serde(rename = "tr")]
    Turkish,
    /// Ukrainian (uk)
    #[serde(rename = "uk")]
    Ukrainian,
    /// Vietnamese (vi)
    #[serde(rename = "vi")]
    Vietnamese,
    /// ChineseSimplified (zh-CN)
    #[serde(rename = "zh-CN")]
    ChineseSimplified,
    /// ChineseTraditional (zh-TW)
    #[serde(rename = "zh-TW")]
    ChineseTraditional,
}

/// Country Code Top-Level Domain
/// From https://icannwiki.org/Country_code_top-level_domain
#[derive(Clone, Copy, Debug, Serialize)]
#[allow(dead_code)]
pub enum Region {
    /// AscensionIsland (.ac)
    #[serde(rename = ".ac")]
    AscensionIsland,
    /// Andorra (.ad)
    #[serde(rename = ".ad")]
    Andorra,
    /// UnitedArabEmirates (.ae)
    #[serde(rename = ".ae")]
    UnitedArabEmirates,
    /// Afghanistan (.af)
    #[serde(rename = ".af")]
    Afghanistan,
    /// AntiguaAndBarbuda (.ag)
    #[serde(rename = ".ag")]
    AntiguaAndBarbuda,
    /// Anguilla (.ai)
    #[serde(rename = ".ai")]
    Anguilla,
    /// Albania (.al)
    #[serde(rename = ".al")]
    Albania,
    /// Armenia (.am)
    #[serde(rename = ".am")]
    Armenia,
    /// AntillesNetherlands (.an)
    #[serde(rename = ".an")]
    AntillesNetherlands,
    /// Angola (.ao)
    #[serde(rename = ".ao")]
    Angola,
    /// Antarctica (.aq)
    #[serde(rename = ".aq")]
    Antarctica,
    /// Argentina (.ar)
    #[serde(rename = ".ar")]
    Argentina,
    /// AmericanSamoa (.as)
    #[serde(rename = ".as")]
    AmericanSamoa,
    /// Austria (.at)
    #[serde(rename = ".at")]
    Austria,
    /// Australia (.au)
    #[serde(rename = ".au")]
    Australia,
    /// Aruba (.aw)
    #[serde(rename = ".aw")]
    Aruba,
    /// AlandIslands (.ax)
    #[serde(rename = ".ax")]
    AlandIslands,
    /// Azerbaijan (.az)
    #[serde(rename = ".az")]
    Azerbaijan,
    /// BosniaAndHerzegovina (.ba)
    #[serde(rename = ".ba")]
    BosniaAndHerzegovina,
    /// Barbados (.bb)
    #[serde(rename = ".bb")]
    Barbados,
    /// Bangladesh (.bd)
    #[serde(rename = ".bd")]
    Bangladesh,
    /// Belgium (.be)
    #[serde(rename = ".be")]
    Belgium,
    /// BurkinaFaso (.bf)
    #[serde(rename = ".bf")]
    BurkinaFaso,
    /// Bulgaria (.bg)
    #[serde(rename = ".bg")]
    Bulgaria,
    /// Bahrain (.bh)
    #[serde(rename = ".bh")]
    Bahrain,
    /// Burundi (.bi)
    #[serde(rename = ".bi")]
    Burundi,
    /// Benin (.bj)
    #[serde(rename = ".bj")]
    Benin,
    /// SaintBarthelemy (.bl)
    #[serde(rename = ".bl")]
    SaintBarthelemy,
    /// Bermuda (.bm)
    #[serde(rename = ".bm")]
    Bermuda,
    /// BruneiDarussalam (.bn)
    #[serde(rename = ".bn")]
    BruneiDarussalam,
    /// Bolivia (.bo)
    #[serde(rename = ".bo")]
    Bolivia,
    /// Bonaire (.bq)
    #[serde(rename = ".bq")]
    BonaireSintEustatiusAndSaba,
    /// Brazil (.br)
    #[serde(rename = ".br")]
    Brazil,
    /// Bahamas (.bs)
    #[serde(rename = ".bs")]
    Bahamas,
    /// Bhutan (.bt)
    #[serde(rename = ".bt")]
    Bhutan,
    /// BouvetIsland (.bv)
    #[serde(rename = ".bv")]
    BouvetIsland,
    /// Botswana (.bw)
    #[serde(rename = ".bw")]
    Botswana,
    /// Belarus (.by)
    #[serde(rename = ".by")]
    Belarus,
    /// Belize (.bz)
    #[serde(rename = ".bz")]
    Belize,
    /// Canada (.ca)
    #[serde(rename = ".ca")]
    Canada,
    /// CocosIslands (.cc)
    #[serde(rename = ".cc")]
    CocosIslands,
    /// DemocraticRepublicOfTheCongo (.cd)
    #[serde(rename = ".cd")]
    DemocraticRepublicOfTheCongo,
    /// CentralAfricanRepublic (.cf)
    #[serde(rename = ".cf")]
    CentralAfricanRepublic,
    /// RepublicOfCongo (.cg)
    #[serde(rename = ".cg")]
    RepublicOfCongo,
    /// Switzerland (.ch)
    #[serde(rename = ".ch")]
    Switzerland,
    /// CoteDivoire (.ci)
    #[serde(rename = ".ci")]
    CoteDivoire,
    /// CookIslands (.ck)
    #[serde(rename = ".ck")]
    CookIslands,
    /// Chile (.cl)
    #[serde(rename = ".cl")]
    Chile,
    /// Cameroon (.cm)
    #[serde(rename = ".cm")]
    Cameroon,
    /// China (.cn)
    #[serde(rename = ".cn")]
    China,
    /// Colombia (.co)
    #[serde(rename = ".co")]
    Colombia,
    /// CostaRica (.cr)
    #[serde(rename = ".cr")]
    CostaRica,
    /// Cuba (.cu)
    #[serde(rename = ".cu")]
    Cuba,
    /// CapeVerde (.cv)
    #[serde(rename = ".cv")]
    CapeVerde,
    /// Curacao (.cw)
    #[serde(rename = ".cw")]
    Curacao,
    /// ChristmasIsland (.cx)
    #[serde(rename = ".cx")]
    ChristmasIsland,
    /// Cyprus (.cy)
    #[serde(rename = ".cy")]
    Cyprus,
    /// CzechRepublic (.cz)
    #[serde(rename = ".cz")]
    CzechRepublic,
    /// Germany (.de)
    #[serde(rename = ".de")]
    Germany,
    /// Djibouti (.dj)
    #[serde(rename = ".dj")]
    Djibouti,
    /// Denmark (.dk)
    #[serde(rename = ".dk")]
    Denmark,
    /// Dominica (.dm)
    #[serde(rename = ".dm")]
    Dominica,
    /// DominicanRepublic (.do)
    #[serde(rename = ".do")]
    DominicanRepublic,
    /// Algeria (.dz)
    #[serde(rename = ".dz")]
    Algeria,
    /// Ecuador (.ec)
    #[serde(rename = ".ec")]
    Ecuador,
    /// Estonia (.ee)
    #[serde(rename = ".ee")]
    Estonia,
    /// Egypt (.eg)
    #[serde(rename = ".eg")]
    Egypt,
    /// WesternSahara (.eh)
    #[serde(rename = ".eh")]
    WesternSahara,
    /// Eritrea (.er)
    #[serde(rename = ".er")]
    Eritrea,
    /// Spain (.es)
    #[serde(rename = ".es")]
    Spain,
    /// Ethiopia (.et)
    #[serde(rename = ".et")]
    Ethiopia,
    /// EuropeanUnion (.eu)
    #[serde(rename = ".eu")]
    EuropeanUnion,
    /// Finland (.fi)
    #[serde(rename = ".fi")]
    Finland,
    /// Fiji (.fj)
    #[serde(rename = ".fj")]
    Fiji,
    /// FalklandIslands (.fk)
    #[serde(rename = ".fk")]
    FalklandIslands,
    /// FederatedStatesOfMicronesia (.fm)
    #[serde(rename = ".fm")]
    FederatedStatesOfMicronesia,
    /// FaroeIslands (.fo)
    #[serde(rename = ".fo")]
    FaroeIslands,
    /// France (.fr)
    #[serde(rename = ".fr")]
    France,
    /// Gabon (.ga)
    #[serde(rename = ".ga")]
    Gabon,
    /// Grenada (.gd)
    #[serde(rename = ".gd")]
    Grenada,
    /// Georgia (.ge)
    #[serde(rename = ".ge")]
    Georgia,
    /// FrenchGuiana (.gf)
    #[serde(rename = ".gf")]
    FrenchGuiana,
    /// Guernsey (.gg)
    #[serde(rename = ".gg")]
    Guernsey,
    /// Ghana (.gh)
    #[serde(rename = ".gh")]
    Ghana,
    /// Gibraltar (.gi)
    #[serde(rename = ".gi")]
    Gibraltar,
    /// Greenland (.gl)
    #[serde(rename = ".gl")]
    Greenland,
    /// Gambia (.gm)
    #[serde(rename = ".gm")]
    Gambia,
    /// Guinea (.gn)
    #[serde(rename = ".gn")]
    Guinea,
    /// Guadeloupe (.gp)
    #[serde(rename = ".gp")]
    Guadeloupe,
    /// EquatorialGuinea (.gq)
    #[serde(rename = ".gq")]
    EquatorialGuinea,
    /// Greece (.gr)
    #[serde(rename = ".gr")]
    Greece,
    /// SouthGeorgiaAndTheSouthSandwichIslands (.gs)
    #[serde(rename = ".gs")]
    SouthGeorgiaAndTheSouthSandwichIslands,
    /// Guatemala (.gt)
    #[serde(rename = ".gt")]
    Guatemala,
    /// Guam (.gu)
    #[serde(rename = ".gu")]
    Guam,
    /// GuineaBissau (.gw)
    #[serde(rename = ".gw")]
    GuineaBissau,
    /// Guyana (.gy)
    #[serde(rename = ".gy")]
    Guyana,
    /// HongKong (.hk)
    #[serde(rename = ".hk")]
    HongKong,
    /// HeardIslandAndMcDonaldIslands (.hm)
    #[serde(rename = ".hm")]
    HeardIslandAndMcDonaldIslands,
    /// Honduras (.hn)
    #[serde(rename = ".hn")]
    Honduras,
    /// Croatia (.hr)
    #[serde(rename = ".hr")]
    Croatia,
    /// Haiti (.ht)
    #[serde(rename = ".ht")]
    Haiti,
    /// Hungary (.hu)
    #[serde(rename = ".hu")]
    Hungary,
    /// Indonesia (.id)
    #[serde(rename = ".id")]
    Indonesia,
    /// Ireland (.ie)
    #[serde(rename = ".ie")]
    Ireland,
    /// Israel (.il)
    #[serde(rename = ".il")]
    Israel,
    /// IsleOfMan (.im)
    #[serde(rename = ".im")]
    IsleOfMan,
    /// India (.in)
    #[serde(rename = ".in")]
    India,
    /// BritishIndianOceanTerritory (.io)
    #[serde(rename = ".io")]
    BritishIndianOceanTerritory,
    /// Iraq (.iq)
    #[serde(rename = ".iq")]
    Iraq,
    /// IslamicRepublicOfIran (.ir)
    #[serde(rename = ".ir")]
    IslamicRepublicOfIran,
    /// Iceland (.is)
    #[serde(rename = ".is")]
    Iceland,
    /// Italy (.it)
    #[serde(rename = ".it")]
    Italy,
    /// Jersey (.je)
    #[serde(rename = ".je")]
    Jersey,
    /// Jamaica (.jm)
    #[serde(rename = ".jm")]
    Jamaica,
    /// Jordan (.jo)
    #[serde(rename = ".jo")]
    Jordan,
    /// Japan (.jp)
    #[serde(rename = ".jp")]
    Japan,
    /// Kenya (.ke)
    #[serde(rename = ".ke")]
    Kenya,
    /// Kyrgyzstan (.kg)
    #[serde(rename = ".kg")]
    Kyrgyzstan,
    /// Cambodia (.kh)
    #[serde(rename = ".kh")]
    Cambodia,
    /// Kiribati (.ki)
    #[serde(rename = ".ki")]
    Kiribati,
    /// Comoros (.km)
    #[serde(rename = ".km")]
    Comoros,
    /// SaintKittsAndNevis (.kn)
    #[serde(rename = ".kn")]
    SaintKittsAndNevis,
    /// DemocraticPeoplesRepublicOfKorea (.kp)
    #[serde(rename = ".kp")]
    DemocraticPeoplesRepublicOfKorea,
    /// RepublicOfKorea (.kp)
    #[serde(rename = ".kp")]
    RepublicOfKorea,
    /// Kuwait (.kw)
    #[serde(rename = ".kw")]
    Kuwait,
    /// CaymenIslands (.ky)
    #[serde(rename = ".ky")]
    CaymenIslands,
    /// Kazakhstan (.kz)
    #[serde(rename = ".kz")]
    Kazakhstan,
    /// Laos (.la)
    #[serde(rename = ".la")]
    Laos,
    /// Lebanon (.lb)
    #[serde(rename = ".lb")]
    Lebanon,
    /// SaintLucia (.lc)
    #[serde(rename = ".lc")]
    SaintLucia,
    /// Liechtenstein (.li)
    #[serde(rename = ".li")]
    Liechtenstein,
    /// SriLanka (.lk)
    #[serde(rename = ".lk")]
    SriLanka,
    /// Liberia (.lr)
    #[serde(rename = ".lr")]
    Liberia,
    /// Lesotho (.ls)
    #[serde(rename = ".ls")]
    Lesotho,
    /// Lithuania (.lt)
    #[serde(rename = ".lt")]
    Lithuania,
    /// Luxembourg (.lu)
    #[serde(rename = ".lu")]
    Luxembourg,
    /// Latvia (.lv)
    #[serde(rename = ".lv")]
    Latvia,
    /// Libya (.ly)
    #[serde(rename = ".ly")]
    Libya,
    /// Morocco (.ma)
    #[serde(rename = ".ma")]
    Morocco,
    /// Monaco (.mc)
    #[serde(rename = ".mc")]
    Monaco,
    /// RepublicOfMoldova (.md)
    #[serde(rename = ".md")]
    RepublicOfMoldova,
    /// Montenegro (.me)
    #[serde(rename = ".me")]
    Montenegro,
    /// SaintMartin (.mf)
    #[serde(rename = ".mf")]
    SaintMartin,
    /// Madagascar (.mg)
    #[serde(rename = ".mg")]
    Madagascar,
    /// MarshallIslands (.mh)
    #[serde(rename = ".mh")]
    MarshallIslands,
    /// Macedonia (.mk)
    #[serde(rename = ".mk")]
    Macedonia,
    /// Mali (.ml)
    #[serde(rename = ".ml")]
    Mali,
    /// Myanmar (.mm)
    #[serde(rename = ".mm")]
    Myanmar,
    /// Mongolia (.mn)
    #[serde(rename = ".mn")]
    Mongolia,
    /// Macao (.mo)
    #[serde(rename = ".mo")]
    Macao,
    /// NorthernMarianaIslands (.mp)
    #[serde(rename = ".mp")]
    NorthernMarianaIslands,
    /// Martinique (.mq)
    #[serde(rename = ".mq")]
    Martinique,
    /// Mauritania (.mr)
    #[serde(rename = ".mr")]
    Mauritania,
    /// Montserrat (.ms)
    #[serde(rename = ".ms")]
    Montserrat,
    /// Malta (.mt)
    #[serde(rename = ".mt")]
    Malta,
    /// Mauritius (.mu)
    #[serde(rename = ".mu")]
    Mauritius,
    /// Maldives (.mv)
    #[serde(rename = ".mv")]
    Maldives,
    /// Malawi (.mw)
    #[serde(rename = ".mw")]
    Malawi,
    /// Mexico (.mx)
    #[serde(rename = ".mx")]
    Mexico,
    /// Malaysia (.my)
    #[serde(rename = ".my")]
    Malaysia,
    /// Mozambique (.mz)
    #[serde(rename = ".mz")]
    Mozambique,
    /// Namibia (.na)
    #[serde(rename = ".na")]
    Namibia,
    /// NewCaledonia (.nc)
    #[serde(rename = ".nc")]
    NewCaledonia,
    /// Niger (.ne)
    #[serde(rename = ".ne")]
    Niger,
    /// NorfolkIsland (.nf)
    #[serde(rename = ".nf")]
    NorfolkIsland,
    /// Nigeria (.ng)
    #[serde(rename = ".ng")]
    Nigeria,
    /// Nicaragua (.ni)
    #[serde(rename = ".ni")]
    Nicaragua,
    /// Netherlands (.nl)
    #[serde(rename = ".nl")]
    Netherlands,
    /// Norway (.no)
    #[serde(rename = ".no")]
    Norway,
    /// Nepal (.np)
    #[serde(rename = ".np")]
    Nepal,
    /// Nauru (.nr)
    #[serde(rename = ".nr")]
    Nauru,
    /// Niue (.nu)
    #[serde(rename = ".nu")]
    Niue,
    /// NewZealand (.nz)
    #[serde(rename = ".nz")]
    NewZealand,
    /// Oman (.om)
    #[serde(rename = ".om")]
    Oman,
    /// Panama (.pa)
    #[serde(rename = ".pa")]
    Panama,
    /// Peru (.pe)
    #[serde(rename = ".pe")]
    Peru,
    /// FrenchPolynesia (.pf)
    #[serde(rename = ".pf")]
    FrenchPolynesia,
    /// PapuaNewGuinea (.pg)
    #[serde(rename = ".pg")]
    PapuaNewGuinea,
    /// Philippines (.ph)
    #[serde(rename = ".ph")]
    Philippines,
    /// Pakistan (.pk)
    #[serde(rename = ".pk")]
    Pakistan,
    /// Poland (.pl)
    #[serde(rename = ".pl")]
    Poland,
    /// SaintPierreAndMiquelon (.pm)
    #[serde(rename = ".pm")]
    SaintPierreAndMiquelon,
    /// Pitcairn (.pn)
    #[serde(rename = ".pn")]
    Pitcairn,
    /// PuertoRico (.pr)
    #[serde(rename = ".pr")]
    PuertoRico,
    /// Palestine (.ps)
    #[serde(rename = ".ps")]
    Palestine,
    /// Portugal (.pt)
    #[serde(rename = ".pt")]
    Portugal,
    /// Palau (.pw)
    #[serde(rename = ".pw")]
    Palau,
    /// Paraguay (.py)
    #[serde(rename = ".py")]
    Paraguay,
    /// Qatar (.qa)
    #[serde(rename = ".qa")]
    Qatar,
    /// Reunion (.re)
    #[serde(rename = ".re")]
    Reunion,
    /// Romania (.ro)
    #[serde(rename = ".ro")]
    Romania,
    /// Serbia (.rs)
    #[serde(rename = ".rs")]
    Serbia,
    /// Russia (.ru)
    #[serde(rename = ".ru")]
    Russia,
    /// Rwanda (.rw)
    #[serde(rename = ".rw")]
    Rwanda,
    /// SaudiArabia (.sa)
    #[serde(rename = ".sa")]
    SaudiArabia,
    /// SolomonIslands (.sb)
    #[serde(rename = ".sb")]
    SolomonIslands,
    /// Seychelles (.sc)
    #[serde(rename = ".sc")]
    Seychelles,
    /// Sudan (.sd)
    #[serde(rename = ".sd")]
    Sudan,
    /// Sweden (.se)
    #[serde(rename = ".se")]
    Sweden,
    /// Singapore (.sg)
    #[serde(rename = ".sg")]
    Singapore,
    /// SaintHelena (.sh)
    #[serde(rename = ".sh")]
    SaintHelena,
    /// Slovenia (.si)
    #[serde(rename = ".si")]
    Slovenia,
    /// SvalbardAndJanMayen (.sj)
    #[serde(rename = ".sj")]
    SvalbardAndJanMayen,
    /// Slovakia (.sk)
    #[serde(rename = ".sk")]
    Slovakia,
    /// SierraLeone (.sl)
    #[serde(rename = ".sl")]
    SierraLeone,
    /// SanMarino (.sm)
    #[serde(rename = ".sm")]
    SanMarino,
    /// Senegal (.sn)
    #[serde(rename = ".sn")]
    Senegal,
    /// Somalia (.so)
    #[serde(rename = ".so")]
    Somalia,
    /// Suriname (.sr)
    #[serde(rename = ".sr")]
    Suriname,
    /// SouthSudan (.ss)
    #[serde(rename = ".ss")]
    SouthSudan,
    /// SaoTomeAndPrincipe (.st)
    #[serde(rename = ".st")]
    SaoTomeAndPrincipe,
    /// SovietUnion (.su)
    #[serde(rename = ".su")]
    SovietUnion,
    /// ElSalvador (.sv)
    #[serde(rename = ".sv")]
    ElSalvador,
    /// SintMaarten (.sx)
    #[serde(rename = ".sx")]
    SintMaarten,
    /// Syria (.sy)
    #[serde(rename = ".sy")]
    Syria,
    /// Swaziland (.sz)
    #[serde(rename = ".sz")]
    Swaziland,
    /// TurksAndCaicosIslands (.tc)
    #[serde(rename = ".tc")]
    TurksAndCaicosIslands,
    /// Chad (.td)
    #[serde(rename = ".td")]
    Chad,
    /// FrenchSouthernTerritories (.tf)
    #[serde(rename = ".tf")]
    FrenchSouthernTerritories,
    /// Togo (.tg)
    #[serde(rename = ".tg")]
    Togo,
    /// Thailand (.th)
    #[serde(rename = ".th")]
    Thailand,
    /// Tajikistan (.tj)
    #[serde(rename = ".tj")]
    Tajikistan,
    /// Tokelau (.tk)
    #[serde(rename = ".tk")]
    Tokelau,
    /// TimorLeste (.tl)
    #[serde(rename = ".tl")]
    TimorLeste,
    /// Turkmenistan (.tm)
    #[serde(rename = ".tm")]
    Turkmenistan,
    /// Tunisia (.tn)
    #[serde(rename = ".tn")]
    Tunisia,
    /// Tonga (.to)
    #[serde(rename = ".to")]
    Tonga,
    /// PortugueseTimor (.tp)
    #[serde(rename = ".tp")]
    PortugueseTimor,
    /// Turkey (.tr)
    #[serde(rename = ".tr")]
    Turkey,
    /// TrinidadAndTobago (.tt)
    #[serde(rename = ".tt")]
    TrinidadAndTobago,
    /// Tuvalu (.tv)
    #[serde(rename = ".tv")]
    Tuvalu,
    /// Taiwan (.tw)
    #[serde(rename = ".tw")]
    Taiwan,
    /// Tanzania (.tz)
    #[serde(rename = ".tz")]
    Tanzania,
    /// Ukraine (.ua)
    #[serde(rename = ".ua")]
    Ukraine,
    /// Uganda (.ug)
    #[serde(rename = ".ug")]
    Uganda,
    /// UnitedKingdom (.uk)
    #[serde(rename = ".uk")]
    UnitedKingdom,
    /// UnitedStatesMinorOutlyingIslands (.um)
    #[serde(rename = ".um")]
    UnitedStatesMinorOutlyingIslands,
    /// UnitedStates (.us)
    #[serde(rename = ".us")]
    UnitedStates,
    /// Uruguay (.uy)
    #[serde(rename = ".uy")]
    Uruguay,
    /// Uzbekistan (.uz)
    #[serde(rename = ".uz")]
    Uzbekistan,
    /// VaticanCity (.va)
    #[serde(rename = ".va")]
    VaticanCity,
    /// SaintVincentAndTheGrenadines (.vc)
    #[serde(rename = ".vc")]
    SaintVincentAndTheGrenadines,
    /// Venezuela (.ve)
    #[serde(rename = ".ve")]
    Venezuela,
    /// BritishVirginIslands (.vg)
    #[serde(rename = ".vg")]
    BritishVirginIslands,
    /// USVirginIslands (.vi)
    #[serde(rename = ".vi")]
    USVirginIslands,
    /// Vietnam (.vn)
    #[serde(rename = ".vn")]
    Vietnam,
    /// Vanuatu (.vu)
    #[serde(rename = ".vu")]
    Vanuatu,
    /// WallisAndFutuna (.wf)
    #[serde(rename = ".wf")]
    WallisAndFutuna,
    /// Samoa (.ws)
    #[serde(rename = ".ws")]
    Samoa,
    /// Mayote (.yt)
    #[serde(rename = ".yt")]
    Mayote,
    /// SouthAfrica (.za)
    #[serde(rename = ".za")]
    SouthAfrica,
    /// Zambia (.zm)
    #[serde(rename = ".zm")]
    Zambia,
    /// Zimbabwe (.zw)
    #[serde(rename = ".zw")]
    Zimbabwe,
}

/// WGS-84 coordinates that support serializing and deserializing
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Coordinates {
    pub lat: f64,
    pub lng: f64,
}

/// A unique identifier that can be used with other Google APIs.
/// For example, you can use the place_id in a Places SDK request to get details of a local business, such as phone number, opening hours, user reviews, and more. See the place ID overview.
#[derive(Debug, Clone, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PlaceId(String);
