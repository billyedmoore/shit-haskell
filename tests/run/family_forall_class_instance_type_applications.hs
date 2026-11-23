{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
class C (a :: k)
instance C @Type Int
main :: IO ()
main = pure ()
