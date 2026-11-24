{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F a
type instance forall . F Int = Bool
main :: IO ()
main = pure ()
