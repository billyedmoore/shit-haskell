{-# LANGUAGE TemplateHaskell #-}
module QQ (str, shout) where
import Language.Haskell.TH
import Language.Haskell.TH.Quote
str :: QuasiQuoter
str = QuasiQuoter { quoteExp = stringE, quotePat = undefined, quoteType = undefined, quoteDec = undefined }
shout :: QuasiQuoter
shout = QuasiQuoter { quoteExp = \s -> appE (varE 'length) (stringE (s ++ "!")), quotePat = undefined, quoteType = undefined, quoteDec = undefined }
