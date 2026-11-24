{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,NoTypeAbstractions #-}
import Data.Kind (Type)
type T (_ :: Type) = Int
main :: IO ()
main = pure ()
