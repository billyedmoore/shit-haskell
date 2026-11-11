module Multi.Report (report, versionText) where

import Data.Version (showVersion)
import Multi.Internal
import Multi.Shapes
import Paths_multi (version)

report :: [Shape] -> [String]
report = map line
  where
    line s = show s ++ ": " ++ classify s ++ " " ++ show (fst (tag ()))

versionText :: String
versionText = "multi " ++ showVersion version ++ ", feature " ++ show feature ++ ", " ++ describeBase
