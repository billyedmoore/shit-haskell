{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
type Keep _ a _ = a
main :: IO ()
main = print (3 :: Keep () Int Bool)
