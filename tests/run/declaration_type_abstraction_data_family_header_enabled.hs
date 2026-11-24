{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
type F :: forall k. k -> Type
data family F @k a
main :: IO ()
main = pure ()
