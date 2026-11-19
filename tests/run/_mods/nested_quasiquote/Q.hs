{-# LANGUAGE TemplateHaskellQuotes #-}
module Q (str, fn) where
import Language.Haskell.TH (litE, stringL)
import Language.Haskell.TH.Quote (QuasiQuoter (..))

str :: QuasiQuoter
str = QuasiQuoter { quoteExp = litE . stringL, quotePat = undefined, quoteType = undefined, quoteDec = undefined }

-- A quoter whose expression is a function (the text appended).
fn :: QuasiQuoter
fn = QuasiQuoter { quoteExp = \t -> [| (++ t) |], quotePat = undefined, quoteType = undefined, quoteDec = undefined }
