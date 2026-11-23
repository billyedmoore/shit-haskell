{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
type F :: forall k. k -> Type
type family F a where
  F @Type Bool = Int
main :: IO ()
main = pure ()
