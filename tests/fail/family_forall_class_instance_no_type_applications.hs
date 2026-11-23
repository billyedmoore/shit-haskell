{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, NoTypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
class C (a :: k)
instance C @Type Int
main :: IO ()
main = pure ()
