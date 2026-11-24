{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
data family F _
data instance F Bool = F Int
main :: IO ()
main = case F 4 of F n -> print n
