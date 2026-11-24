{-# LANGUAGE TypeFamilies,TypeFamilyDependencies,TypeAbstractions,NoTypeApplications #-}
class C a where
  type F @k a = r | r -> a
main :: IO ()
main = pure ()
