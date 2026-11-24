{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications,TypeOperators #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family a :+ b
type instance a :+ Proxy @Type b = Int
main :: IO ()
main = pure ()
