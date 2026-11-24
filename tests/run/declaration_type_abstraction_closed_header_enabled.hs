{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
type F :: forall k. k -> Type
type family F @k a where
  F Int = Bool
main :: IO ()
main = pure ()
