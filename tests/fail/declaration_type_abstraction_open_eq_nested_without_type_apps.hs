{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F a
type instance F (Proxy @Type Bool) = Int
main :: IO ()
main = pure ()
