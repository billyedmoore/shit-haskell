{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
data family F (a :: k)
data instance F @Type Bool = F
main :: IO ()
main = pure ()
