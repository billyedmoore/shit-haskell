module Q (str) where
import Language.Haskell.TH (litE, stringL)
import Language.Haskell.TH.Quote (QuasiQuoter (..))

str :: QuasiQuoter
str = QuasiQuoter { quoteExp = litE . stringL, quotePat = undefined, quoteType = undefined, quoteDec = undefined }
