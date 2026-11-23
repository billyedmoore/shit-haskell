{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,NamedWildCards,NoTypeAbstractions #-}
import Data.Kind (Type)
type T _a = _a
main :: IO ()
main = print (10 :: T Int)
