{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
type family F (a :: k)
type instance F @Type Bool = Int
main :: IO ()
main = pure ()
